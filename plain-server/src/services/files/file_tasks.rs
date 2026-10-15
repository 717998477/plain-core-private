use super::platform::FilePlatform;
use anyhow::{Result, anyhow, bail};
use plain_rs::db::Db;
use plain_rs::events::WsEvent;
use plain_rs::files::tasks::CompletedOp;
use plain_rs::files::tasks::Events;
use plain_rs::files::tasks::FileTask;
use plain_rs::files::tasks::FileTaskOp;
use plain_rs::files::tasks::FileTaskStatus;
use plain_rs::files::tasks::FileTaskType;
use plain_rs::files::tasks::HookResult;
use plain_rs::files::tasks::Hooks;
use plain_rs::files::tasks::PrepareResult;
use plain_rs::files::tasks::Service;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
pub struct FileTasks {
    store: Arc<plain_rs::kv::LazyDb>,
    deletion_lock: tokio::sync::Mutex<()>,
    events: broadcast::Sender<WsEvent>,
    service: Mutex<Option<Arc<Service>>>,
    hooks: Arc<NativeHooks>,
}
impl FileTasks {
    pub(crate) fn new(
        store: Arc<plain_rs::kv::LazyDb>,
        platform: Arc<FilePlatform>,
        events: broadcast::Sender<WsEvent>,
        audio: Arc<crate::services::audio::host::Audio>,
        index: Arc<crate::services::images::index::ImageIndex>,
        prefs: Arc<plain_rs::prefs::Prefs>,
        duration_cache: Arc<plain_rs::media::duration_cache::DurationCache>,
    ) -> Self {
        Self {
            store,
            hooks: Arc::new(NativeHooks {
                platform,
                audio,
                index,
                prefs,
                duration_cache,
            }),
            deletion_lock: tokio::sync::Mutex::new(()),
            events,
            service: Mutex::new(None),
        }
    }
    fn service(&self) -> Result<Arc<Service>> {
        let mut service = self
            .service
            .lock()
            .map_err(|_| anyhow!("file task service poisoned"))?;
        if let Some(service) = service.as_ref() {
            return Ok(service.clone());
        }
        tokio::runtime::Handle::try_current()?;
        let next = Arc::new(Service::with_hooks(
            Arc::new(Changes(self.events.clone())),
            self.hooks.clone(),
        ));
        *service = Some(next.clone());
        Ok(next)
    }
    pub async fn create(
        &self,
        client_id: &str,
        kind: FileTaskType,
        title: &str,
        ops: Vec<FileTaskOp>,
    ) -> Result<FileTask> {
        if client_id.is_empty() {
            bail!("unauthorized");
        }
        if ops.is_empty()
            || ops.iter().any(|op| {
                !PathBuf::from(&op.src).is_absolute() || !PathBuf::from(&op.dst).is_absolute()
            })
        {
            bail!("absolute file paths required");
        }
        self.hooks.authorize(kind, &ops).await?;
        self.service()?.create(client_id, kind, title, ops)
    }
    pub async fn execute(&self, kind: FileTaskType, ops: Vec<FileTaskOp>) -> Result<bool> {
        let client_id = self
            .hooks
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("file task identity unavailable"))?;
        let task = self
            .create(&client_id, kind, &format!("{kind:?}").to_uppercase(), ops)
            .await?;
        let service = self.service()?;
        loop {
            let current = match service.get(&task.id)? {
                Some(current) => current,
                None => self
                    .list(client_id.clone())
                    .await?
                    .into_iter()
                    .find(|row| row.id == task.id)
                    .ok_or_else(|| anyhow!("file task unavailable"))?,
            };
            match current.status {
                FileTaskStatus::Done => return Ok(true),
                FileTaskStatus::Error => return Ok(false),
                _ => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            }
        }
    }
    pub async fn remove(&self, client_id: String, id: String) -> Result<bool> {
        let service = self.service()?;
        tokio::task::spawn_blocking(move || service.remove(&client_id, &id)).await?
    }
    pub async fn list(&self, client_id: String) -> Result<Vec<FileTask>> {
        let service = self.service()?;
        tokio::task::spawn_blocking(move || service.list(&client_id)).await?
    }
}
struct Changes(broadcast::Sender<WsEvent>);
impl Events for Changes {
    fn changed(&self, task: &FileTask) {
        if matches!(task.status, FileTaskStatus::Done | FileTaskStatus::Error) {
            let _ = self
                .0
                .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
        }
    }
}
struct NativeHooks {
    duration_cache: Arc<plain_rs::media::duration_cache::DurationCache>,
    platform: Arc<FilePlatform>,
    audio: Arc<crate::services::audio::host::Audio>,
    index: Arc<crate::services::images::index::ImageIndex>,
    prefs: Arc<plain_rs::prefs::Prefs>,
}
impl Hooks for NativeHooks {
    fn prepare<'a>(&'a self, kind: FileTaskType, op: &'a FileTaskOp) -> PrepareResult<'a> {
        Box::pin(async move {
            if kind == FileTaskType::Move {
                crate::services::files::file_task_media::prepare(&self.platform, &op.src).await
            } else {
                Ok(serde_json::Value::Null)
            }
        })
    }
    fn completed_with_snapshot<'a>(
        &'a self,
        kind: FileTaskType,
        op: &'a CompletedOp,
        snapshot: &'a serde_json::Value,
    ) -> HookResult<'a> {
        self.apply_completed(kind, op, snapshot)
    }
    fn authorize<'a>(&'a self, kind: FileTaskType, ops: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async move { self.platform.authorize_task(kind, ops).await })
    }
    fn completed<'a>(&'a self, kind: FileTaskType, op: &'a CompletedOp) -> HookResult<'a> {
        self.scan_completed(kind, op)
    }
}
impl NativeHooks {
    fn apply_completed<'a>(
        &'a self,
        kind: FileTaskType,
        op: &'a CompletedOp,
        snapshot: &'a serde_json::Value,
    ) -> HookResult<'a> {
        Box::pin(async move {
            if kind == FileTaskType::Move {
                let source = crate::services::files::file_task_media::source_root(snapshot)?;
                let destination = crate::services::files::file_task_media::normalized(
                    std::path::Path::new(&op.dst),
                )
                .await?;
                self.duration_cache.invalidate_paths(
                    &[],
                    &[op.src.clone(), op.dst.clone(), source, destination],
                )?;
            }

            self.completed(kind, op).await?;
            if kind == FileTaskType::Move {
                let change = crate::services::files::file_task_media::resolve(
                    &self.platform,
                    &op.dst,
                    snapshot,
                )
                .await?;
                let removed_sources = change
                    .bindings
                    .iter()
                    .map(|binding| binding.source_path.clone())
                    .collect::<Vec<_>>();
                let raw_source = op.src.clone();
                let raw_destination = op.dst.clone();
                let prefs = self.prefs.clone();
                let index = self.index.clone();
                self.audio
                    .run(move |db, engine| {
                        use plain_rs::files::moves as media_moves;
                        use plain_rs::media::audio::playback as audio_playback;
                        let before = audio_playback::snapshot(db)?;
                        let source_prefix =
                            format!("{}/", change.source_root.trim_end_matches('/'));
                        let moved_audio = !before.path.is_empty()
                            && (before.path == change.source_root
                                || before.path.starts_with(&source_prefix)
                                || before.path == raw_source
                                || before.path.starts_with(&format!(
                                    "{}/",
                                    raw_source.trim_end_matches('/')
                                )));
                        let rebind = |db: &Db| {
                            media_moves::rebind_with_aliases(
                                db,
                                &change.bindings,
                                &change.source_root,
                                &change.destination_root,
                                &[(raw_source, raw_destination)],
                            )
                        };
                        index.update_cache(rebind)?;
                        if moved_audio {
                            use plain_rs::media::audio::commands;
                            commands::clear_engine(db, &prefs, engine)?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(|e| anyhow!(e.message))?;
                self.platform.forget_removed(removed_sources).await?;
            }
            Ok(())
        })
    }
    fn scan_completed<'a>(&'a self, kind: FileTaskType, op: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async move {
            let root = PathBuf::from(&op.dst);
            let mut walker = crate::services::files::file_task_walk::FileWalker::new(&root);
            let mut batch = Vec::new();
            while let Some(path) = walker.next().await? {
                let info = tokio::fs::symlink_metadata(&path).await?;
                if info.is_file() {
                    batch.push(
                        path.to_str()
                            .ok_or_else(|| anyhow!("invalid path encoding"))?
                            .to_owned(),
                    );
                    if kind == FileTaskType::Move {
                        let relative = path.strip_prefix(&root)?;
                        let source = if relative.as_os_str().is_empty() {
                            PathBuf::from(&op.src)
                        } else {
                            PathBuf::from(&op.src).join(relative)
                        };
                        batch.push(
                            source
                                .to_str()
                                .ok_or_else(|| anyhow!("invalid path encoding"))?
                                .to_owned(),
                        );
                    }
                    if batch.len() >= 128 {
                        self.platform.scan(&batch).await?;
                        batch.clear();
                    }
                }
            }
            if !batch.is_empty() {
                self.platform.scan(&batch).await?;
            }
            Ok(())
        })
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/services/files/file_tasks.rs"]
mod tests;

#[cfg(feature = "core")]
pub mod mutations;

mod trash;
