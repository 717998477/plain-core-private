use crate::host::bridge::Host;
use anyhow::{Result, anyhow, bail};
use plain_rs::db::Db;
use plain_rs::files::tasks::{FileTaskOp, FileTaskType};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) struct FilePlatform {
    host: Arc<Host>,
    db: Arc<Db>,
    directory: PathBuf,
    #[cfg(test)]
    sdk: std::sync::atomic::AtomicBool,
    #[cfg(feature = "media")]
    scans: Arc<super::media_scans::MediaScans>,
}
impl FilePlatform {
    pub(crate) fn new(
        host: Arc<Host>,
        db: Arc<Db>,
        directory: PathBuf,
        #[cfg(feature = "media")] scans: Arc<super::media_scans::MediaScans>,
    ) -> Self {
        Self {
            host,
            db,
            directory,
            #[cfg(test)]
            sdk: std::sync::atomic::AtomicBool::new(cfg!(any(
                target_os = "android",
                target_os = "ios"
            ))),
            #[cfg(feature = "media")]
            scans,
        }
    }
    pub(crate) fn uses_sdk(&self) -> bool {
        #[cfg(test)]
        {
            self.sdk.load(std::sync::atomic::Ordering::Relaxed)
        }
        #[cfg(not(test))]
        {
            cfg!(any(target_os = "android", target_os = "ios"))
        }
    }

    #[cfg(test)]
    pub(crate) fn use_sdk_for_test(&self) {
        self.sdk.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) async fn thumbnail_cache(&self, path: &str) -> Result<PathBuf> {
        if self.uses_sdk() {
            let value = self
                .host
                .call("thumbnailAuthorize", json!({"paths":[path]}))
                .await
                .map_err(anyhow::Error::msg)?;
            let path = PathBuf::from(
                value
                    .as_str()
                    .ok_or_else(|| anyhow!("invalid thumbnail cache root"))?,
            );
            if !path.is_absolute() {
                bail!("absolute thumbnail cache root required");
            }
            return Ok(path);
        }
        self.authorize(path, false).await?;
        #[cfg(feature = "media")]
        if let Some(directory) = self.scans.cache_directory().map_err(anyhow::Error::msg)? {
            return Ok(directory);
        }
        Ok(self.directory.join("cache"))
    }

    pub(crate) async fn recent_files(
        &self,
        prefs: &plain_rs::prefs::Prefs,
    ) -> Result<Vec<serde_json::Value>> {
        if self.uses_sdk() {
            return serde_json::from_value(
                self.host
                    .call("systemRecentFileFacts", json!({}))
                    .await
                    .map_err(anyhow::Error::msg)?,
            )
            .map_err(Into::into);
        }
        let mut items = Vec::new();
        for path in plain_rs::prefs::recent::get_recent_files(prefs, 500)? {
            let Ok(entry) = plain_rs::files::filesystem::stat(Path::new(&path)).await else {
                continue;
            };
            items.push(json!({"name":Path::new(&entry.path).file_name().and_then(|name| name.to_str()).unwrap_or_default(),
                "path":entry.path,"permission":"","mediaId":null,"createdAt":entry.created_at.timestamp_millis(),
                "updatedAt":entry.updated_at.timestamp_millis(),"size":entry.size,"isDir":entry.is_dir,"childCount":entry.child_count}));
        }
        Ok(items)
    }

    pub(crate) async fn indexed_files(
        &self,
        query: String,
        offset: i32,
        limit: i32,
        sort: crate::graphql::resolvers::files::types::FileSortBy,
        count_only: bool,
    ) -> Result<serde_json::Value> {
        if self.uses_sdk() {
            return super::indexed::platform(
                &self.db, &self.host, &query, offset, limit, sort, count_only,
            )
            .await;
        }
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            super::indexed::native(&db, &query, offset, limit, sort, count_only)
        })
        .await?
    }
    async fn receipt(&self, method: &str, params: serde_json::Value) -> Result<()> {
        if self
            .host
            .call(method, params)
            .await
            .map_err(anyhow::Error::msg)?
            .as_bool()
            != Some(true)
        {
            bail!("invalid file platform receipt");
        }
        Ok(())
    }
    pub(crate) async fn authorize_task(
        &self,
        kind: FileTaskType,
        ops: &[FileTaskOp],
    ) -> Result<()> {
        if self.uses_sdk() {
            return self.receipt("fileTaskAuthorize", json!({"type":kind,"paths":ops.iter().flat_map(|op| [&op.src,&op.dst]).collect::<Vec<_>>()})).await;
        }
        for op in ops {
            self.authorize(&op.src, kind == FileTaskType::Move).await?;
            self.authorize(&op.dst, true).await?;
        }
        Ok(())
    }
    pub(crate) async fn authorize(&self, path: &str, write: bool) -> Result<()> {
        self.authorize_paths(&[path.to_owned()], write).await
    }
    pub(crate) async fn authorize_paths(&self, paths: &[String], write: bool) -> Result<()> {
        if self.uses_sdk() {
            let params = if write {
                json!({"paths":paths})
            } else {
                json!({"operation":"read","paths":paths})
            };
            return self.receipt("fileTaskAuthorize", params).await;
        }
        let paths = paths.to_vec();
        tokio::task::spawn_blocking(move || {
            for path in paths {
                authorize_native(Path::new(&path), write)?;
            }
            Ok(())
        })
        .await?
    }
    pub(crate) async fn identities(
        &self,
        paths: &[String],
    ) -> Result<Vec<super::file_task_media::Identity>> {
        if self.uses_sdk() {
            return Ok(serde_json::from_value(
                self.host
                    .call("fileTaskMediaSnapshot", json!({"paths": paths}))
                    .await
                    .map_err(anyhow::Error::msg)?,
            )?);
        }
        let db = self.db.clone();
        let paths = serde_json::to_string(paths)?;
        Ok(tokio::task::spawn_blocking(move || db.with_conn(|connection| {
            connection.prepare("SELECT data_type,id,path FROM media_items WHERE data_type IN (1,2,3,24) AND path IN (SELECT value FROM json_each(?1))")?
                .query_map([paths], |row| Ok(super::file_task_media::Identity { media_type: row.get(0)?, media_id: row.get(1)?, path: row.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })).await??)
    }
    pub(crate) async fn scan(&self, paths: &[String]) -> Result<()> {
        if self.uses_sdk() {
            return self.receipt("fileTaskScan", json!({"paths":paths})).await;
        }
        #[cfg(feature = "media")]
        {
            let db = self.db.clone();
            let scans = self.scans.clone();
            let directory = self.directory.clone();
            let paths = paths.to_vec();
            tokio::task::spawn_blocking(move || {
                let mut existing = Vec::new();
                for path in paths {
                    match std::fs::metadata(&path) {
                        Ok(metadata) if metadata.is_file() => existing.push(path),
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.to_string()),
                    }
                }
                super::native_scan::completed(&db, &scans, &directory, existing)
            })
            .await?
            .map_err(anyhow::Error::msg)?;
        }
        Ok(())
    }
    pub(crate) async fn forget_removed(&self, paths: Vec<String>) -> Result<()> {
        if self.uses_sdk() {
            return Ok(());
        }
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            for path in paths {
                if !Path::new(&path).try_exists()? {
                    db.forget_media_file(&path)?;
                }
            }
            anyhow::Ok(())
        })
        .await?
    }
}
pub(super) fn authorize_native(path: &Path, write: bool) -> Result<()> {
    if !path.is_absolute() {
        bail!("absolute file path required");
    }
    if write {
        plain_rs::files::filesystem::deletion::validate(path)?;
        let mut ancestor = path
            .parent()
            .ok_or_else(|| anyhow!("file parent required"))?;
        while !ancestor.try_exists()? {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| anyhow!("existing file ancestor required"))?;
        }
        let ancestor = ancestor.canonicalize()?;
        if !ancestor.is_dir() {
            bail!("writable file parent required");
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let name = std::ffi::CString::new(ancestor.as_os_str().as_bytes())?;
            if unsafe { libc::access(name.as_ptr(), libc::W_OK | libc::X_OK) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    } else {
        let path = path.canonicalize()?;
        if path.is_file() {
            std::fs::File::open(path)?;
        } else if path.is_dir() {
            std::fs::read_dir(path)?;
        } else {
            bail!("regular file or directory required");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/platform.rs"]
mod tests;
