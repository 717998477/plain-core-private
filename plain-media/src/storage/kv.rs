//! Embedded KV store (fjall) replacing the Go `cockroachdb/pebble` usage.
//!
//! The whole app treats the database as one flat, byte-ordered key-value
//! namespace with key prefixes (`tag:`, `event:`, `session:`, ...), so we
//! expose exactly that surface backed by a single fjall keyspace and hide
//! the engine behind [`Db`].

use anyhow::Result;

use std::path::Path;
use std::sync::OnceLock;

mod events;
mod keyvalue;
#[allow(dead_code)]
mod signature_key;
pub mod storage_alias;
mod url_token;

pub use events::EventLog;
pub use keyvalue::{device_display_name, server_client_id};
pub use signature_key::SignatureKey;
pub use storage_alias as storage;
pub use url_token::UrlToken;

pub use plain_rs::kv::{Batch, Db, default_db_path};

static DB: OnceLock<Db> = OnceLock::new();

/// Open (or return the cached) default database.
pub fn open(path: &Path) -> Result<&'static Db> {
    if let Some(db) = DB.get() {
        return Ok(db);
    }
    let db = Db::open(path)?;
    Ok(DB.get_or_init(|| db))
}

/// Install an already-opened [`Db`] as the process-global default (first
/// caller wins; later calls return the incumbent). Hosts that open the
/// store themselves (tests with temp dirs, `MediaService::init`) use
/// this so `get_default()` callers share their handle.
pub fn set_default(db: Db) -> &'static Db {
    DB.get_or_init(move || db)
}

/// Returns the cached default database. Panics if `open` was not called yet.
pub fn get_default() -> &'static Db {
    DB.get()
        .expect("db::open must be called before get_default")
}

/// Non-panicking variant for background paths that may run before the
/// database is open — e.g. the automount watcher's event emission.
pub fn try_get_default() -> Option<&'static Db> {
    DB.get()
}
