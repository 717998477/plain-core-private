use super::*;
use plain_rs::files::filesystem::deletion::{Outcome, Plan};
use plain_rs::files::trash::{SortOrder, Trash, TrashItem};

impl FileTasks {
    fn trash_store(&self) -> Result<Trash> {
        Ok(Trash::new(self.store.get()?))
    }
    pub(crate) async fn trash_paths(&self, paths: Vec<String>) -> Result<usize> {
        let _guard = self.deletion_lock.lock().await;
        let trash = self.trash_store()?;
        let mut count = 0;
        for path in paths {
            self.hooks.platform.authorize(&path, true).await?;
            let root =
                super::super::file_task_media::normalized(std::path::Path::new(&path)).await?;
            let checked = root.clone();
            let plan =
                tokio::task::spawn_blocking(move || Plan::inspect(std::path::Path::new(&checked)))
                    .await??;
            let snapshot = super::super::file_task_media::deletion_snapshot(
                &self.hooks.platform,
                root.clone(),
                &plan.files,
            )
            .await?;
            let paths = plan.paths();
            trash.trash_paths(vec![root.clone()]).await?;
            let outcome = Outcome {
                removed: true,
                paths,
                failures: Vec::new(),
            };
            self.finish_deletion(&path, &root, &snapshot, &outcome)
                .await?;
            count += 1;
        }
        Ok(count)
    }
    pub(crate) async fn restore_paths(&self, references: Vec<String>) -> Result<usize> {
        let _guard = self.deletion_lock.lock().await;
        let trash = self.trash_store()?;
        let mut count = 0;
        for reference in references {
            let item = trash
                .resolve_item(&reference)?
                .ok_or_else(|| anyhow!("trash item not found"))?;
            self.hooks
                .platform
                .authorize(&item.original_path, true)
                .await?;
            let physical = Trash::physical_path(&item).to_string_lossy().into_owned();
            self.hooks.platform.authorize(&physical, false).await?;
            for restored in trash.restore_paths(vec![item.id]).await? {
                self.hooks
                    .scan_completed(
                        FileTaskType::Copy,
                        &CompletedOp {
                            src: physical.clone(),
                            dst: restored,
                        },
                    )
                    .await?;
                count += 1;
            }
        }
        let _ = self
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
        Ok(count)
    }
    pub(crate) async fn delete_trashed(&self, reference: String) -> Result<bool> {
        let _guard = self.deletion_lock.lock().await;
        let trash = self.trash_store()?;
        let Some(item) = trash.resolve_item(&reference)? else {
            return Ok(false);
        };
        self.hooks
            .platform
            .authorize(&Trash::physical_path(&item).to_string_lossy(), true)
            .await?;
        trash.delete_trash_by_path(&item.id).await?;
        let _ = self
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
        Ok(true)
    }
    pub(crate) async fn trashed(
        &self,
        offset: usize,
        limit: usize,
        query: String,
        order: SortOrder,
    ) -> Result<Vec<TrashItem>> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let trash = Trash::new(store.get()?);
            let mut rows = trash.list_trash(offset, limit, &query, order)?;
            for row in &mut rows {
                plain_rs::files::trash::fill_stats(row)?;
            }
            Ok(rows)
        })
        .await?
    }
    pub(crate) async fn trash_count(&self) -> Result<usize> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || Trash::new(store.get()?).trash_count()).await?
    }
}
