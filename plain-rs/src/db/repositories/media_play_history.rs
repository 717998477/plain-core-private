use crate::db::{Db, MediaPlayHistoryRow};
use crate::enums::DataType;
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaPlayHistoryRow> {
    Ok(MediaPlayHistoryRow {
        media_id: row.get(0)?,
        position_ms: row.get(1)?,
        play_count: row.get(2)?,
        played_at: row.get(3)?,
        media_type: row.get(4)?,
    })
}

pub(crate) fn record_play(
    conn: &Connection,
    media_type: DataType,
    media_id: &str,
    position_ms: i64,
) -> rusqlite::Result<()> {
    let current = conn
        .query_row(
            "SELECT play_count FROM media_play_history WHERE media_id=?1 AND media_type=?2",
            params![media_id, media_type.kind()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    let count = current.checked_add(1).ok_or_else(|| {
        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
            "play count overflow",
        )))
    })?;
    let changed = conn.execute("INSERT INTO media_play_history(media_id,position_ms,play_count,played_at,media_type) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(media_id) DO UPDATE SET position_ms=excluded.position_ms,play_count=excluded.play_count,played_at=excluded.played_at WHERE media_play_history.media_type=excluded.media_type", params![media_id,position_ms,count,crate::utils::dbtime::now_iso_millis(),media_type.kind()])?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

impl Db {
    pub fn media_play_history_get(
        &self,
        media_type: DataType,
        media_id: &str,
    ) -> rusqlite::Result<Option<MediaPlayHistoryRow>> {
        self.with_conn(|conn| conn.query_row("SELECT media_id,position_ms,play_count,played_at,media_type FROM media_play_history WHERE media_id=?1 AND media_type=?2", params![media_id,media_type.kind()], from_row).optional())
    }

    pub fn media_play_history_start(
        &self,
        media_type: DataType,
        media_id: &str,
        position_ms: i64,
    ) -> rusqlite::Result<()> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            record_play(&tx, media_type, media_id, position_ms)?;
            tx.commit()
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/db/repositories/media_play_history.rs"]
mod tests;
