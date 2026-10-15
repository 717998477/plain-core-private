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
        let inner = Arc::new(Inner { database, keyspace });
        opened.insert(path, Arc::downgrade(&inner));
        Ok(Self(inner))
    }

    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Slice>> {
        Ok(self.0.keyspace.get(key.as_ref())?)
    }

    pub fn insert<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> Result<()> {
        self.0
            .keyspace
            .insert(key.as_ref(), value.as_ref())
            .context("fjall insert")?;
        Ok(())
    }

    pub fn remove<K: AsRef<[u8]>>(&self, key: K) -> Result<()> {
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
    pub fn flush(&self) -> Result<()> {
        self.0
            .database
            .persist(fjall::PersistMode::SyncData)
            .context("fjall persist")?;
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
        batch.commit().context("fjall batch commit")?;
        Ok(())
    }
}
pub struct LazyDb {
    path: PathBuf,
    db: Mutex<Option<Arc<Db>>>,
}

impl LazyDb {
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
    data_dir.join("fjall")
}

#[cfg(test)]
#[path = "../tests/unit/kv.rs"]
mod tests;
