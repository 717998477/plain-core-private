use crate::kv::Db;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PREFIX: &str = "android_media_duration:";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Duration {
    pub media_type: String,
    pub media_id: String,
    pub path: String,
    pub size_bytes: i64,
    pub modified_at: String,
    pub duration_ms: i64,
    pub updated_at: String,
}

pub struct DurationCache {
    directory: PathBuf,
    db: Mutex<Option<Db>>,
}

impl DurationCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            directory: crate::kv::default_db_path(data_dir),
            db: Mutex::new(None),
        }
    }

    fn with_db<T>(&self, run: impl FnOnce(&Db) -> Result<T>) -> Result<T> {
        let mut db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("Duration cache poisoned"))?;
        if db.is_none() {
            *db = Some(Db::open(&self.directory)?);
        }
        run(db.as_ref().unwrap())
    }

    pub fn all(&self) -> Result<Vec<Duration>> {
        self.with_db(|db| {
            db.scan_prefix(PREFIX)
                .map(|entry| {
                    let (_, value) = entry?;
                    Ok(serde_json::from_slice(&value)?)
                })
                .collect()
        })
    }

    pub fn save(&self, mut value: Duration) -> Result<()> {
        let key = key(&value.media_type, &value.media_id)?;
        if value.path.is_empty() || value.size_bytes < 0 || value.duration_ms <= 0 {
            bail!("Invalid media duration cache entry");
        }
        value.modified_at = chrono::DateTime::parse_from_rfc3339(&value.modified_at)?
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        value.updated_at = crate::utils::dbtime::now_iso_millis();
        self.with_db(|db| {
            db.insert(key, serde_json::to_vec(&value)?)?;
            db.flush()
        })
    }

    pub fn remove(&self, media_type: &str, ids: &[String]) -> Result<usize> {
        let keys = ids
            .iter()
            .map(|id| key(media_type, id))
            .collect::<Result<Vec<_>>>()?;
        if ids.is_empty() {
            return Ok(0);
        }
        self.with_db(|db| {
            let mut batch = db.batch();
            let mut removed = std::collections::HashSet::new();
            for key in keys {
                if db.get(&key)?.is_some() && removed.insert(key.clone()) {
                    batch.remove(key);
                }
            }
            db.apply_batch(batch)?;
            db.flush()?;
            Ok(removed.len())
        })
    }

    pub fn invalidate_paths(&self, paths: &[String], roots: &[String]) -> Result<()> {
        if paths.is_empty() && roots.is_empty() {
            return Ok(());
        }
        self.with_db(|db| {
            let mut batch = db.batch();
            for entry in db.scan_prefix(PREFIX) {
                let (key, value) = entry?;
                let value: Duration = serde_json::from_slice(&value)?;
                let path = Path::new(&value.path);
                if paths.iter().any(|item| path == Path::new(item))
                    || roots.iter().any(|root| path.starts_with(root))
                {
                    batch.remove(key);
                }
            }
            db.apply_batch(batch)?;
            db.flush()
        })
    }
}

fn key(media_type: &str, id: &str) -> Result<String> {
    if !matches!(media_type, "audio" | "video") || id.is_empty() {
        bail!("Invalid media duration identity");
    }
    Ok(format!("{PREFIX}{media_type}:{id}"))
}

#[cfg(test)]
#[path = "../../tests/unit/media/duration_cache.rs"]
mod tests;
