use crate::host::bridge::Host;
use crate::services::audio::library::HostLibrary;
use plain_rs::db::Db;
use plain_rs::error::DomainResult;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

pub struct Audio {
    db: Arc<Db>,
    host: Arc<Host>,
    lock: Arc<Mutex<()>>,
    capacity: Arc<Semaphore>,
    resources: Arc<crate::host::resources::NativeResources>,
    cache: std::path::PathBuf,
}
enum Preparation {
    References,
    Current,
    None,
}
impl Audio {
    pub fn new(
        db: Arc<Db>,
        host: Arc<Host>,
        resources: Arc<crate::host::resources::NativeResources>,
        cache: std::path::PathBuf,
    ) -> Self {
        Self {
            db,
            host,
            lock: Arc::new(Mutex::new(())),
            capacity: Arc::new(Semaphore::new(32)),
            resources,
            cache,
        }
    }
    pub async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Db, &mut HostLibrary) -> DomainResult<T> + Send + 'static,
    ) -> async_graphql::Result<T> {
        self.execute(Preparation::References, operation).await
    }

    pub async fn run_playback<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Db, &mut HostLibrary) -> DomainResult<T> + Send + 'static,
    ) -> async_graphql::Result<T> {
        self.execute(Preparation::Current, operation).await
    }

    pub async fn metadata(
        &self,
        path: String,
    ) -> async_graphql::Result<plain_rs::media::audio::queue::AudioTrack> {
        self.execute(Preparation::None, move |_, library| {
            plain_rs::media::audio::commands::Engine::metadata(library, &path)
        })
        .await
    }

    pub(crate) async fn cover(&self, path: &str) -> Result<Option<std::path::PathBuf>, String> {
        super::covers::cache(&self.resources, &self.cache, path).await
    }

    async fn execute<T: Send + 'static>(
        &self,
        preparation: Preparation,
        operation: impl FnOnce(&Db, &mut HostLibrary) -> DomainResult<T> + Send + 'static,
    ) -> async_graphql::Result<T> {
        let db = self.db.clone();
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| async_graphql::Error::new("audio request capacity exceeded"))?;
        let guard = self.lock.clone().lock_owned().await;
        let mut library = HostLibrary::new(
            self.host.clone(),
            db.clone(),
            self.resources.clone(),
            self.cache.clone(),
        );
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _guard = guard;
            match preparation {
                Preparation::References => library.refresh()?,
                Preparation::Current => library.restore_current()?,
                Preparation::None => {}
            }
            operation(&db, &mut library)
        })
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?
        .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
}
