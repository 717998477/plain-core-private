use super::playback::Playback;
use crate::db::Db;
use crate::db::{QueueSource, QueueSourceKind};
use crate::error::{DomainError, DomainResult};
use std::sync::{Arc, Mutex};

const SOURCE_PREF: &str = "audio_queue_source";

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct StoredSource {
    source: QueueSourceKind,
    playlist_id: String,
    current_media_id: Option<String>,
    current_index: i64,
    sort_by: String,
}

impl From<&State> for StoredSource {
    fn from(state: &State) -> Self {
        Self {
            source: state.source.source,
            playlist_id: state.source.playlist_id.clone(),
            current_media_id: state.current_media_id.clone(),
            current_index: state.source.current_index,
            sort_by: state.source.sort_by.clone(),
        }
    }
}

use std::sync::atomic::{AtomicI64, Ordering};

static NEXT_REVISION: AtomicI64 = AtomicI64::new(1);

#[derive(Clone)]
pub(crate) struct State {
    pub playback: Playback,
    pub source: QueueSource,
    pub current_media_id: Option<String>,
    pub load_revision: i64,
    pub started_revision: i64,
}

impl State {
    fn new(path: String) -> Self {
        Self {
            playback: Playback {
                path,
                ..Playback::default()
            },
            source: QueueSource::default(),
            current_media_id: None,
            load_revision: -1,
            started_revision: -1,
        }
    }

    pub fn advance(&mut self) -> DomainResult<()> {
        self.playback.revision = NEXT_REVISION
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |revision| {
                revision.checked_add(1)
            })
            .map_err(|_| DomainError::Other("audio revision overflow".into()))?;
        Ok(())
    }

    pub fn select(&mut self, path: &str) -> DomainResult<()> {
        self.source.current_path = path.to_owned();
        if path.is_empty() {
            self.current_media_id = None;
        }
        if self.playback.path != path {
            self.advance()?;
            self.playback.path = path.into();
            self.playback.position_ms = 0;
            self.load_revision = -1;
            self.started_revision = -1;
        }
        Ok(())
    }
}

pub(crate) struct Runtime {
    state: Mutex<State>,
    restored: Mutex<bool>,
    prefs: Arc<crate::prefs::Prefs>,
}

impl Runtime {
    pub fn new(prefs: Arc<crate::prefs::Prefs>) -> DomainResult<Self> {
        let mut state = State::new(String::new());
        if let Some(stored) = prefs
            .get::<StoredSource>(SOURCE_PREF)
            .map_err(|error| DomainError::Other(error.to_string()))?
        {
            state.source.source = stored.source;
            state.source.playlist_id = stored.playlist_id;
            state.source.current_index = stored.current_index;
            state.source.sort_by = stored.sort_by;
            state.current_media_id = stored.current_media_id;
        }
        Ok(Self {
            state: Mutex::new(state),
            restored: Mutex::new(false),
            prefs,
        })
    }

    pub fn restore_needed(&self) -> bool {
        !*self.restored.lock().unwrap()
    }

    pub fn restore(&self, path: String) -> DomainResult<()> {
        let mut restored = self.restored.lock().unwrap();
        if !*restored {
            let mut state = self.state.lock().unwrap();
            if state.playback.path.is_empty() {
                state.select(&path)?;
            }
            *restored = true;
        }
        Ok(())
    }

    pub fn read<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(&self.state.lock().unwrap())
    }

    pub fn update<T>(&self, update: impl FnOnce(&mut State) -> DomainResult<T>) -> DomainResult<T> {
        let mut state = self.state.lock().unwrap();
        let mut next = state.clone();
        let result = update(&mut next)?;
        *state = next;
        Ok(result)
    }

    pub fn transaction<T>(
        &self,
        db: &Db,
        operation: impl FnOnce(&rusqlite::Transaction<'_>, &mut State) -> DomainResult<T>,
    ) -> DomainResult<T> {
        let mut state = self.state.lock().unwrap();
        let mut next = state.clone();
        let result = db.with_conn(|connection| -> DomainResult<T> {
            let transaction = connection.unchecked_transaction()?;
            let result = operation(&transaction, &mut next)?;
            let previous = StoredSource::from(&*state);
            let stored = StoredSource::from(&next);
            let saved = self
                .prefs
                .get::<StoredSource>(SOURCE_PREF)
                .map_err(|error| DomainError::Other(error.to_string()))?;
            if stored != previous {
                self.prefs
                    .set(SOURCE_PREF, &stored)
                    .map_err(|error| DomainError::Other(error.to_string()))?;
            }
            if let Err(error) = transaction.commit() {
                if stored != previous {
                    match saved {
                        Some(saved) => self.prefs.set(SOURCE_PREF, &saved),
                        None => self.prefs.remove(SOURCE_PREF),
                    }
                    .map_err(|restore| {
                        DomainError::Other(format!("{error}; restore queue preferences: {restore}"))
                    })?;
                }
                return Err(error.into());
            }
            Ok(result)
        })?;
        *state = next;
        Ok(result)
    }
}
