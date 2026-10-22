use rusqlite::Connection;
mod state;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(any(
    feature = "chat",
    feature = "library",
    feature = "sqlite_browse",
    feature = "sqlite_browse"
))]
mod repositories;
#[cfg(feature = "chat")]
pub(crate) use repositories::channel::{CHANNEL_COLS, row_to_channel};
#[cfg(feature = "chat")]
pub(crate) use repositories::chat::{CHAT_COLS, row_to_chat};
#[cfg(feature = "chat")]
pub(crate) use repositories::peer::{PEER_COLS, row_to_peer};
#[cfg(any(feature = "chat", feature = "library", feature = "sqlite_browse"))]
mod models;
#[cfg(any(feature = "chat", feature = "library"))]
mod schema;

#[cfg(feature = "sqlite_browse")]
pub use repositories::browse;
#[cfg(feature = "sqlite_browse")]
pub use repositories::devtools;
#[cfg(feature = "library")]
pub use repositories::{
    audio_catalog, audio_queue, favorite_folder, image_catalog, image_editor_project,
    media_play_history, notes_feeds, tag,
};
#[cfg(feature = "chat")]
pub use repositories::{bookmark, chat_store};

#[cfg(all(feature = "chat", feature = "sqlite_browse"))]
pub use crate::sqlite_browse::TableColumnMeta;
#[cfg(feature = "library")]
pub use models::{
    ArchivedConversationRow, AuditEventRow, ClipboardRow, FavoriteFolderRow, HISTORY_KEEP,
    ImageEmbeddingRow, MediaPlayHistoryRow, PlayHistory, Playlist, PlaylistItem, PomodoroItemRow,
    QueueItem, SessionRow, ShareRow, TagRelationRow, TagRow, TrashedMessageRow,
    VideoPlayProgressRow,
};
#[cfg(feature = "chat")]
pub use models::{DAppFile, DChannel, DChat, DNearbyDeviceCache, DPeer};
#[cfg(feature = "chat")]
pub use repositories::db_time::{iso_from_unix_millis, now_iso, now_millis, short_id};

#[derive(Clone)]
pub struct Db(
    Arc<Mutex<Connection>>,
    Arc<state::State>,
    #[cfg(feature = "library")] Arc<crate::media::audio::state::Runtime>,
    #[cfg(feature = "library")] Arc<crate::media::audio::state::Runtime>,
);

impl Db {
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        #[cfg(feature = "library")]
        {
            let store = if db_path == Path::new(":memory:") { None } else {
                Some(Arc::new(crate::kv::Db::open(&crate::data_paths::DataPaths::new(
                    db_path.parent().unwrap_or(Path::new("."))).kv())
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?))
            };
            Self::open_audio(db_path, store)
        }
        #[cfg(not(feature = "library"))]
        {
            let connection = Self::open_connection(db_path)?;
            let state = state::State::new(&connection);
            Ok(Self(Arc::new(Mutex::new(connection)), Arc::new(state)))
        }
    }

    #[cfg(feature = "library")]
    pub fn open_with_kv(db_path: &Path, store: Arc<crate::kv::Db>) -> rusqlite::Result<Self> {
        Self::open_audio(db_path, Some(store))
    }

    #[cfg(feature = "library")]
    fn open_audio(db_path: &Path, store: Option<Arc<crate::kv::Db>>) -> rusqlite::Result<Self> {
        let connection = Self::open_connection(db_path)?;
        let state = state::State::new(&connection);
        let runtime = |key| crate::media::audio::state::Runtime::new(store.clone(), key)
            .map(Arc::new).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)));
        Ok(Self(Arc::new(Mutex::new(connection)), Arc::new(state),
            runtime("local_audio_current_media_id")?, runtime("remote_audio_current_media_id")?))
    }

    #[cfg(feature = "library")]
    // The queue and library stay shared; only playback state changes.
    pub fn remote_audio(&self) -> Self {
        Self(self.0.clone(), self.1.clone(), self.3.clone(), self.2.clone())
    }

    #[cfg(feature = "library")]
    pub fn audio_selected_ids(&self) -> Vec<String> {
        [&self.2, &self.3].iter()
            .filter_map(|runtime| runtime.read(|state| state.current_media_id.clone())).collect()
    }

    #[cfg(feature = "library")]
    pub fn key_values(&self) -> Option<Arc<crate::kv::Db>> {
        self.audio().key_values()
    }

    #[cfg(feature = "library")]
    pub(crate) fn audio(&self) -> &crate::media::audio::state::Runtime {
        &self.2
    }

    #[cfg(feature = "library")]
    pub(crate) fn audio_transaction<T>(
        &self,
        operation: impl FnOnce(
            &rusqlite::Transaction<'_>,
            &mut crate::media::audio::state::State,
        ) -> crate::error::DomainResult<T>,
    ) -> crate::error::DomainResult<T> {
        self.audio().transaction(self, operation)
    }

    fn open_connection(db_path: &Path) -> rusqlite::Result<Connection> {
        if let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error {
                        code: rusqlite::ffi::ErrorCode::CannotOpen,
                        extended_code: 0,
                    },
                    Some(format!(
                        "failed to create database parent dir {}: {e}",
                        parent.display()
                    )),
                )
            })?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;",
        )?;
        #[cfg(any(feature = "chat", feature = "library"))]
        schema::init(&conn)?;
        #[cfg(feature = "library")]
        repositories::audio_catalog::init(&conn)?;
        #[cfg(feature = "library")]
        repositories::image_catalog::init(&conn)?;
        Ok(conn)
    }

    #[cfg(feature = "chat")]
    pub(crate) fn app_files_lock(&self) -> std::io::Result<std::sync::MutexGuard<'_, ()>> {
        self.1
            .app_files
            .lock()
            .map_err(|_| std::io::Error::other("app file store lock poisoned"))
    }

    pub fn backup_to(&self, path: &Path, cancelled: impl Fn() -> bool) -> Result<(), String> {
        if cancelled() {
            return Err("Backup cancelled".into());
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        let result = self.with_conn(|source| {
            let mut destination = Connection::open(path).map_err(|error| error.to_string())?;
            {
                let backup = rusqlite::backup::Backup::new(source, &mut destination)
                    .map_err(|error| error.to_string())?;
                loop {
                    if cancelled() {
                        return Err("Backup cancelled".into());
                    }
                    match backup.step(128).map_err(|error| error.to_string())? {
                        rusqlite::backup::StepResult::Done => break,
                        rusqlite::backup::StepResult::More => {}
                        _ => return Err("Database is busy during backup".into()),
                    }
                }
            }
            destination
                .close()
                .map_err(|(_, error)| error.to_string())?;
            std::fs::File::open(path)
                .and_then(|file| file.sync_all())
                .map_err(|error| error.to_string())
        });
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    #[cfg(feature = "chat")]
    pub fn peer_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.1.peers.subscribe()
    }

    pub fn with_conn<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&Connection) -> T,
    {
        let conn = self.0.lock().unwrap();
        f(&conn)
    }
}

#[cfg(feature = "chat")]
impl Db {
    pub fn table_columns(&self, table: &str) -> Vec<TableColumnMeta> {
        self.with_conn(|conn| crate::sqlite_browse::table_columns(conn, table))
    }

    pub fn primary_key_column(&self, table: &str) -> String {
        const FALLBACK: &str = "id";
        self.with_conn(|conn| {
            crate::sqlite_browse::primary_key_column(conn, table)
                .unwrap_or_else(|| FALLBACK.to_string())
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/db/mod.rs"]
mod tests;

#[cfg(all(test, feature = "chat"))]
#[path = "../../tests/unit/db/mod.rs"]
mod chat_tests;
#[cfg(all(test, feature = "library"))]
#[path = "../../tests/unit/db/mod.rs"]
mod library_tests;

#[cfg(feature = "library")]
pub use models::{
    MediaFileMetadata, MediaFileMetadataAvailability, MediaFileMetadataRead, MediaFileRow,
};

#[cfg(feature = "library")]
pub use repositories::media_file;

#[cfg(all(test, feature = "chat", feature = "library"))]
#[path = "../../tests/unit/db/design.rs"]
mod design_tests;

#[cfg(test)]
#[path = "../../tests/unit/db/backup.rs"]
mod backup_tests;
