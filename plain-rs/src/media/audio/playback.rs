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
    db.restore_audio_current()?;
    Ok(db.audio().read(|state| state.playback.clone()))
}

pub fn invalidate(db: &Db) -> DomainResult<Playback> {
    db.restore_audio_current()?;
    db.audio().update(|state| {
        state.advance()?;
        Ok(state.playback.clone())
    })
}

pub fn seek(db: &Db, position_ms: i64) -> DomainResult<Playback> {
    db.restore_audio_current()?;
    if position_ms < 0 {
        return Err(DomainError::Other("negative audio position".into()));
    }
    db.audio_transaction(|tx, state| {
        if state.playback.path.is_empty() {
            return Err(DomainError::Other("no selected audio".into()));
        }
        state.advance()?;
        state.playback.position_ms = position_ms;
        save_position(tx, &state.playback.path, position_ms)?;
        Ok(state.playback.clone())
    })
}

pub fn report(db: &Db, path: &str, revision: i64, position_ms: i64) -> DomainResult<bool> {
    if revision < 0 || position_ms < 0 || path.is_empty() {
        return Err(DomainError::Other("invalid audio progress".into()));
    }
    db.audio_transaction(|tx, state| {
        if state.playback.path != path || state.playback.revision != revision {
            return Ok(false);
        }
        state.playback.position_ms = position_ms;
        save_position(tx, path, position_ms)?;
        Ok(true)
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportResult {
    pub accepted: bool,
    pub history_changed: bool,
}

pub fn report_selection(db: &Db, report: &Playback, started: bool) -> DomainResult<ReportResult> {
    if report.path.is_empty() || report.revision < 0 || report.position_ms < 0 {
        return Err(DomainError::Other("invalid audio playback report".into()));
    }
    db.restore_audio_current()?;
    db.audio_transaction(|tx, state| {
        if state.playback.path != report.path || state.playback.revision != report.revision {
            return Ok(ReportResult::default());
        }
        state.playback.position_ms = report.position_ms;
        let history_changed = if started && state.started_revision != report.revision {
            let track = crate::media::audio::queue::track_by_path_conn(tx, &report.path)?
                .unwrap_or_else(|| {
                    crate::media::audio::queue::AudioTrack::from_path_stem(&report.path)
                });
            record_started(tx, state, &track, report.revision)?;
            track.media_id.is_some()
        } else {
            save_position(tx, &report.path, report.position_ms)?;
            false
        };
        Ok(ReportResult {
            accepted: true,
            history_changed,
        })
    })
}

pub fn prepare_track(
    db: &Db,
    track: &crate::media::audio::queue::AudioTrack,
) -> DomainResult<Playback> {
    db.audio_transaction(|tx, state| {
        crate::db::audio_queue::io::save_current(tx, &track.path, state)?;
        state.select(&track.path)?;
        state.advance()?;
        state.playback.position_ms = 0;
        state.load_revision = -1;
        state.started_revision = -1;
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
        record_started(tx, state, track, revision)?;
        Ok(true)
    })
}

fn record_started(
    tx: &rusqlite::Connection,
    state: &mut crate::media::audio::state::State,
    track: &crate::media::audio::queue::AudioTrack,
    revision: i64,
) -> DomainResult<()> {
    crate::media::audio::queue::record_history_conn(
        tx,
        &track.path,
        &track.title,
        &track.artist,
        track.duration_ms,
    )?;
    save_position(tx, &track.path, state.playback.position_ms)?;
    state.started_revision = revision;
    Ok(())
}

fn save_position(
    conn: &rusqlite::Connection,
    path: &str,
    position_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute("UPDATE media_play_history SET position_ms=?1,played_at=?2 WHERE media_id=(SELECT media_id FROM audio_media WHERE path=?3 UNION SELECT a.media_id FROM platform_audio_aliases a JOIN audio_media m USING(media_id) WHERE a.path=?3)",rusqlite::params![position_ms,crate::utils::dbtime::now_iso_millis(),path])?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/media/audio/playback.rs"]
mod tests;
