use crate::db::Db;
use crate::error::{DomainError, DomainResult};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub path: String,
    pub position_ms: i64,
    pub revision: i64,
}

pub fn snapshot(db: &Db) -> DomainResult<Playback> {
    Ok(db.audio().read(|state| state.playback.clone()))
}

pub fn invalidate(db: &Db) -> DomainResult<Playback> {
    db.audio().update(|state| {
        state.advance()?;
        Ok(state.playback.clone())
    })
}

pub fn seek(db: &Db, position_ms: i64) -> DomainResult<Playback> {
    if position_ms < 0 {
        return Err(DomainError::Other("negative audio position".into()));
    }
    db.audio().update(|state| {
        if state.playback.path.is_empty() {
            return Err(DomainError::Other("no selected audio".into()));
        }
        state.advance()?;
        state.playback.position_ms = position_ms;
        Ok(state.playback.clone())
    })
}

pub fn report(db: &Db, path: &str, revision: i64, position_ms: i64) -> DomainResult<bool> {
    if revision < 0 || position_ms < 0 || path.is_empty() {
        return Err(DomainError::Other("invalid audio progress".into()));
    }
    db.audio().update(|state| {
        if state.playback.path != path || state.playback.revision != revision {
            return Ok(false);
        }
        state.playback.position_ms = position_ms;
        Ok(true)
    })
}

pub fn prepare_track(
    db: &Db,
    track: &crate::media::audio::queue::AudioTrack,
    enqueue: bool,
) -> DomainResult<Playback> {
    db.audio_transaction(|tx, state| {
        if enqueue {
            crate::media::audio::queue::enqueue_conn(tx, std::slice::from_ref(track), false)?;
        }
        let mut source = crate::db::audio_queue::io::get_source(tx, state)?;
        source.current_path = track.path.clone();
        crate::db::audio_queue::io::save_source(tx, &source, state)?;
        state.advance()?;
        state.playback.position_ms = 0;
        Ok(state.playback.clone())
    })
}

pub fn loaded(db: &Db, path: &str, revision: i64) -> DomainResult<()> {
    db.audio().update(|state| {
        if path.is_empty() || state.playback.path != path || state.playback.revision != revision {
            return Err(DomainError::Other(
                "stale engine load acknowledgement".into(),
            ));
        }
        state.load_revision = revision;
        state.started_revision = -1;
        Ok(())
    })
}

pub fn started(
    db: &Db,
    track: &crate::media::audio::queue::AudioTrack,
    revision: i64,
) -> DomainResult<bool> {
    if track.path.is_empty() || track.duration_ms < 0 || revision < 0 {
        return Err(DomainError::Other("invalid playback start".into()));
    }
    db.audio_transaction(|tx, state| {
        if state.playback.path != track.path
            || state.load_revision != revision
            || state.started_revision == revision
        {
            return Ok(false);
        }
        crate::media::audio::queue::record_history_conn(
            tx,
            &track.path,
            &track.title,
            &track.artist,
            track.duration_ms,
        )?;
        state.started_revision = revision;
        Ok(true)
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/media/audio/playback.rs"]
mod tests;
