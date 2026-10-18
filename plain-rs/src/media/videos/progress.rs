use crate::db::Db;
use crate::db::VideoPlayProgressRow;

pub fn save(
    db: &Db,
    media_id: &str,
    position_ms: i64,
) -> crate::error::DomainResult<VideoPlayProgressRow> {
    if media_id.trim().is_empty() || position_ms < 0 {
        return Err(crate::error::DomainError::Other(
            "invalid video progress".into(),
        ));
    }
    let row = VideoPlayProgressRow {
        media_id: media_id.into(),
        position_ms,
        updated_at: crate::utils::dbtime::now_iso_millis(),
    };
    db.video_progress_upsert(&row)?;
    Ok(row)
}

pub fn started(
    db: &Db,
    media_id: &str,
    position_ms: i64,
) -> crate::error::DomainResult<VideoPlayProgressRow> {
    if media_id.trim().is_empty() || position_ms < 0 {
        return Err(crate::error::DomainError::Other(
            "invalid video playback".into(),
        ));
    }
    db.media_play_history_start(crate::enums::DataType::Video, media_id, position_ms)?;
    db.video_progress_get(media_id)?
        .ok_or_else(|| crate::error::DomainError::Other("video history unavailable".into()))
}

pub fn recent(
    db: &Db,
    since: chrono::DateTime<chrono::Utc>,
) -> crate::error::DomainResult<Vec<VideoPlayProgressRow>> {
    Ok(db.video_progress_recent(&since.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))?)
}

#[cfg(test)]
#[path = "../../../tests/unit/media/videos/progress.rs"]
mod tests;
