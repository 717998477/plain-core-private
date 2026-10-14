use crate::host::bridge::Host;
use plain_rs::db::{Db, audio_catalog::AudioMedia};
use plain_rs::error::DomainError;
use plain_rs::error::DomainResult;
use plain_rs::media::audio::queue::AudioTrack;
use plain_rs::media::audio::queue::LibraryTracks;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct HostLibrary {
    host: Arc<Host>,
    db: Arc<Db>,
    runtime: tokio::runtime::Handle,
}
impl HostLibrary {
    pub fn new(host: Arc<Host>, db: Arc<Db>) -> Self {
        Self {
            host,
            db,
            runtime: tokio::runtime::Handle::current(),
        }
    }
    pub fn refresh(&mut self) -> DomainResult<()> {
        if cfg!(feature = "mobile") {
            self.db.use_platform_audio_catalog()?;
            let ids = self.db.audio_reference_ids()?;
            let rows: Vec<AudioMedia> = if ids.is_empty() {
                Vec::new()
            } else {
                self.call("audioLibraryResolve", json!({"ids":ids,"paths":[]}))?
            };
            self.db.cache_platform_audio(&rows, true)?;
        }
        self.db.restore_audio_current()?;
        Ok(())
    }

    pub fn restore_current(&mut self) -> DomainResult<()> {
        if cfg!(feature = "mobile") {
            self.db.use_platform_audio_catalog()?;
            if self.db.audio_restore_needed() {
                let id = self.db.audio_current_media_id();
                if let Some(id) = id.filter(|id| !id.is_empty()) {
                    let rows: Vec<AudioMedia> =
                        self.call("audioLibraryResolve", json!({"ids":[id],"paths":[]}))?;
                    self.db.cache_platform_audio(&rows, false)?;
                }
            }
        }
        self.db.restore_audio_current()?;
        Ok(())
    }

    pub fn cache_paths(&mut self, paths: &[String]) -> DomainResult<()> {
        if cfg!(feature = "mobile") && !paths.is_empty() {
            self.db.forget_platform_audio_paths(paths)?;
            let rows: Vec<AudioMedia> =
                self.call("audioLibraryResolve", json!({"ids":[],"paths":paths}))?;
            self.db.cache_platform_audio(&rows, false)?;
        }
        Ok(())
    }

    fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> DomainResult<T> {
        let result = self
            .runtime
            .block_on(self.host.call(method, params))
            .map_err(DomainError::Other)?;
        serde_json::from_value(result)
            .map_err(|e| DomainError::Other(format!("invalid audio provider response: {e}")))
    }
}
impl LibraryTracks for HostLibrary {
    fn library_count(&mut self) -> DomainResult<usize> {
        self.call("audioLibraryCount", json!({}))
    }
    fn library_path_at(&mut self, offset: usize, sort_by: &str) -> DomainResult<Option<String>> {
        self.call(
            "audioLibraryPath",
            json!({"offset":offset,"sortBy":sort_by}),
        )
    }
    fn library_tracks_page(
        &mut self,
        offset: usize,
        limit: usize,
        sort_by: &str,
    ) -> DomainResult<Vec<AudioTrack>> {
        let rows: Vec<AudioMedia> = self.call(
            "audioLibraryPage",
            json!({"offset":offset,"limit":limit,"sortBy":sort_by}),
        )?;
        if cfg!(feature = "mobile") {
            self.db.cache_platform_audio(&rows, false)?;
        }
        Ok(rows.into_iter().map(|row| row.track).collect())
    }
    fn library_locate(&mut self, path: &str, sort_by: &str) -> DomainResult<i64> {
        self.call("audioLibraryLocate", json!({"path":path,"sortBy":sort_by}))
    }
    fn library_contains(&mut self, path: &str) -> DomainResult<bool> {
        self.call("audioLibraryContains", json!({"path":path}))
    }
}

impl plain_rs::media::audio::commands::Engine for HostLibrary {
    fn metadata(&mut self, path: &str) -> DomainResult<AudioTrack> {
        let row: AudioMedia = self.call("audioMetadata", json!({"path":path}))?;
        if cfg!(feature = "mobile") {
            self.db
                .cache_platform_audio(std::slice::from_ref(&row), false)?;
        }
        let mut track = row.track;
        track.path = path.to_owned();
        Ok(track)
    }
    fn execute(
        &mut self,
        command: &plain_rs::media::audio::commands::EngineCommand,
    ) -> DomainResult<plain_rs::media::audio::commands::EngineReport> {
        self.call(
            "audioEngineCommand",
            serde_json::to_value(command).map_err(|e| DomainError::Other(e.to_string()))?,
        )
    }
}
