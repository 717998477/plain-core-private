use super::playback::Playback;
use crate::db::Db;
use crate::error::{DomainError, DomainResult};
use crate::prefs::{Prefs, system::keys::AUDIO_CURRENT_PATH};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_REVISION: AtomicI64 = AtomicI64::new(1);

#[derive(Clone)]
pub(crate) struct State {
    pub playback: Playback,
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
    prefs: Option<Arc<Prefs>>,
    state: Mutex<State>,
}

impl Runtime {
    pub fn new(prefs: Arc<Prefs>) -> crate::prefs::Result<Self> {
        let path = prefs.get(AUDIO_CURRENT_PATH)?.unwrap_or_default();
        Ok(Self {
            prefs: Some(prefs),
            state: Mutex::new(State::new(path)),
        })
    }

    pub fn in_memory() -> Self {
        Self {
            prefs: None,
            state: Mutex::new(State::new(String::new())),
        }
    }

    pub fn read<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(&self.state.lock().unwrap())
    }

    pub fn update<T>(&self, update: impl FnOnce(&mut State) -> DomainResult<T>) -> DomainResult<T> {
        let mut state = self.state.lock().unwrap();
        let mut next = state.clone();
        let result = update(&mut next)?;
        if next.playback.path != state.playback.path {
            self.save_path(&next.playback.path)?;
        }
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
            let changed = next.playback.path != state.playback.path;
            if changed {
                self.save_path(&next.playback.path)?;
            }
            if let Err(error) = transaction.commit() {
                if changed {
                    self.save_path(&state.playback.path)?;
                }
                return Err(error.into());
            }
            Ok(result)
        })?;
        *state = next;
        Ok(result)
    }

    fn save_path(&self, path: &str) -> DomainResult<()> {
        if let Some(prefs) = &self.prefs {
            prefs
                .set(AUDIO_CURRENT_PATH, path)
                .map_err(|error| DomainError::Other(error.to_string()))?;
        }
        Ok(())
    }
}
