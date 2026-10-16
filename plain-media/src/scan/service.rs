//! Host bootstrap for the media stack: one call that installs the
//! process-global media paths, opens the fjall store and hands the host
//! everything the media GraphQL roots and the thumbnail-serving `/fs`
//! branch need. the server wires this at `cmd::run`; the desktop's local
//! server wires it at startup.

use std::path::Path;
use std::sync::Arc;

use crate::storage::kv::Db;

pub struct MediaService {
    /// The fjall store backing media rows, trash, events and sessions.
    pub db: Arc<Db>,
}

impl MediaService {
    pub fn init(data_dir: &Path, cache_dir: &Path) -> anyhow::Result<Self> {
        crate::storage::paths::set(data_dir.to_path_buf(), cache_dir.to_path_buf());
        let db = Db::open(&plain_rs::data_paths::DataPaths::new(data_dir).kv())?;
        let _ = crate::storage::kv::set_default(db.clone());
        Ok(Self { db: Arc::new(db) })
    }
}
