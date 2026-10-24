use anyhow::{Context, Result};
use fjall::Slice;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
const KEYSPACE: &str = "default";
#[derive(Clone)]
pub struct Db(Arc<Inner>);

struct Inner {
    database: fjall::Database,
    json_lock: Mutex<()>,
    #[cfg(test)]
    write_failure: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    write_attempts: std::sync::atomic::AtomicUsize,
    keyspace: fjall::Keyspace,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path).with_context(|| format!("mkdir {}", path.display()))?;
        let path = std::fs::canonicalize(path)?;
        static OPEN: OnceLock<Mutex<HashMap<PathBuf, Weak<Inner>>>> = OnceLock::new();
        let mut opened = OPEN
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| anyhow::anyhow!("KV database registry poisoned"))?;
        if let Some(inner) = opened.get(&path).and_then(Weak::upgrade) {
            return Ok(Self(inner));
        }
        let database = fjall::Database::builder(&path)
            .open()
            .with_context(|| format!("open fjall at {}", path.display()))?;
        let keyspace = database
            .keyspace(KEYSPACE, fjall::KeyspaceCreateOptions::default)
            .context("open fjall keyspace")?;
        let inner = Arc::new(Inner {
            database,
            keyspace,
            json_lock: Mutex::new(()),
            #[cfg(test)]
            write_failure: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            write_attempts: std::sync::atomic::AtomicUsize::new(0),
        });
        opened.insert(path, Arc::downgrade(&inner));
        Ok(Self(inner))
    }

    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Slice>> {
        Ok(self.0.keyspace.get(key.as_ref())?)
    }

    pub fn insert<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> Result<()> {
        #[cfg(test)]
        self.check_write_for_test()?;
        self.0
            .keyspace
            .insert(key.as_ref(), value.as_ref())
            .context("fjall insert")?;
        Ok(())
    }

    pub fn remove<K: AsRef<[u8]>>(&self, key: K) -> Result<()> {
        #[cfg(test)]
        self.check_write_for_test()?;
        self.0
            .keyspace
            .remove(key.as_ref())
            .context("fjall remove")?;
        Ok(())
    }
    pub fn scan_prefix<'a>(
        &'a self,
        prefix: impl AsRef<[u8]> + 'a,
    ) -> impl Iterator<Item = Result<(Slice, Slice)>> + 'a {
        self.0
            .keyspace
            .prefix(prefix.as_ref())
            .map(|guard| guard.into_inner().map_err(Into::into))
    }
    pub fn iter(&self) -> impl Iterator<Item = Result<(Slice, Slice)>> + '_ {
        self.0
            .keyspace
            .iter()
            .map(|guard| guard.into_inner().map_err(Into::into))
    }
    pub fn get_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.get(key)?.map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into)).transpose()
    }
    pub fn set_json<T: serde::Serialize>(&self, key: &str, value: T) -> Result<bool> {
        let _lock = self.0.json_lock.lock().unwrap();
        let value = serde_json::to_vec(&value)?;
        if self.get(key)?.as_deref() == Some(value.as_slice()) { return Ok(false); }
        self.insert(key, value)?;
        self.flush()?;
        Ok(true)
    }
    pub fn update_json<T: serde::Serialize + serde::de::DeserializeOwned + Default, R>(
        &self, key: &str, change: impl FnOnce(&mut T) -> Result<R>,
    ) -> Result<R> {
        let _lock = self.0.json_lock.lock().unwrap();
        let mut value = self.get_json(key)?.unwrap_or_default();
        let result = change(&mut value)?;
        self.insert(key, serde_json::to_vec(&value)?)?;
        self.flush()?;
        Ok(result)
    }
    pub fn update_json_prefix<R>(&self, prefix: &str, change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<R>) -> Result<R> {
        let _lock = self.0.json_lock.lock().unwrap();
        let mut values = serde_json::Map::new();
        for entry in self.scan_prefix(prefix) {
            let (key, value) = entry?;
            values.insert(std::str::from_utf8(&key)?.to_owned(), serde_json::from_slice(&value)?);
        }
        let previous = values.clone();
        let result = change(&mut values)?;
        let mut batch = self.batch();
        for key in previous.keys().filter(|key| !values.contains_key(*key)) { batch.remove(key); }
        for (key, value) in &values {
            anyhow::ensure!(key.starts_with(prefix), "Invalid state key");
            if previous.get(key) != Some(value) { batch.insert(key, serde_json::to_vec(value)?); }
        }
        self.apply_batch(batch)?;
        self.flush()?;
        Ok(result)
    }
    pub fn backup_to(&self, path: &Path, cancelled: impl Fn() -> bool) -> Result<()> {
        if cancelled() {
            anyhow::bail!("Backup cancelled");
        }
        std::fs::create_dir(path)?;
        let result = (|| {
            use fjall::Readable;
            let snapshot = self.0.database.snapshot();
            let destination = Self::open(path)?;
            let mut batch = destination.batch_with_capacity(512);
            let mut pending = 0;
            for entry in snapshot.iter(&self.0.keyspace) {
                if cancelled() {
                    anyhow::bail!("Backup cancelled");
                }
                let (key, value) = entry.into_inner()?;
                batch.insert(&key, &value);
                pending += 1;
                if pending == 512 {
                    destination.apply_batch(batch)?;
                    batch = destination.batch_with_capacity(512);
                    pending = 0;
                }
            }
            if pending != 0 {
                destination.apply_batch(batch)?;
            }
            if cancelled() {
                anyhow::bail!("Backup cancelled");
            }
            destination.flush()
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(path);
        }
        result
    }
    pub fn flush(&self) -> Result<()> {
        #[cfg(test)]
        self.check_write_for_test()?;
        self.0
            .database
            .persist(fjall::PersistMode::SyncData)
            .context("fjall persist")?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn set_write_failure_for_test(&self, failure: bool) {
        self.0.write_failure.store(failure, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn write_attempts_for_test(&self) -> usize {
        self.0.write_attempts.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    fn check_write_for_test(&self) -> Result<()> {
        self.0.write_attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        anyhow::ensure!(!self.0.write_failure.load(std::sync::atomic::Ordering::Relaxed), "injected KV write failure");
        Ok(())
    }
    pub fn batch(&self) -> Batch {
        self.batch_with_capacity(16)
    }
    pub fn batch_with_capacity(&self, items: usize) -> Batch {
        Batch {
            inner: fjall::OwnedWriteBatch::with_capacity(self.0.database.clone(), items),
            store: self.0.clone(),
        }
    }
    pub fn apply_batch(&self, batch: Batch) -> Result<()> {
        #[cfg(test)]
        self.check_write_for_test()?;
        batch.commit().context("fjall batch commit")?;
        Ok(())
    }
}
pub struct LazyDb {
    path: PathBuf,
    db: Mutex<Option<Arc<Db>>>,
}

impl LazyDb {
    pub fn from_open(path: PathBuf, db: Arc<Db>) -> Self {
        Self { path, db: Mutex::new(Some(db)) }
    }

    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            db: Mutex::new(None),
        }
    }

    pub fn get(&self) -> Result<Arc<Db>> {
        let mut stored = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("KV store poisoned"))?;
        if stored.is_none() {
            *stored = Some(Arc::new(Db::open(&self.path)?));
        }
        Ok(stored.as_ref().unwrap().clone())
    }
}

pub struct Batch {
    inner: fjall::OwnedWriteBatch,
    store: Arc<Inner>,
}

impl Batch {
    pub fn insert(&mut self, key: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {
        self.inner
            .insert(&self.store.keyspace, key.as_ref(), value.as_ref());
    }

    pub fn remove(&mut self, key: impl AsRef<[u8]>) {
        self.inner.remove(&self.store.keyspace, key.as_ref());
    }

    fn commit(self) -> Result<()> {
        Ok(self.inner.commit()?)
    }
}

pub fn default_db_path(data_dir: &Path) -> PathBuf {
    crate::data_paths::DataPaths::new(data_dir).kv()
}

#[cfg(test)]
#[path = "../tests/unit/kv.rs"]
mod tests;
