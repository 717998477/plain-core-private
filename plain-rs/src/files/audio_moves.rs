use rusqlite::{Connection, OptionalExtension, params};

pub(crate) struct References {
    queue_order: Option<i64>,
    playlist_items: Vec<(String, String, i64, String)>,
}

pub(crate) fn snapshot(conn: &Connection, media_id: &str) -> rusqlite::Result<References> {
    let queue_order = conn
        .query_row(
            "SELECT sort_order FROM audio_queue_items WHERE media_id=?1",
            [media_id],
            |row| row.get(0),
        )
        .optional()?;
    let playlist_items = conn
        .prepare(
            "SELECT id,playlist_id,sort_order,added_at FROM audio_playlist_items WHERE media_id=?1",
        )?
        .query_map([media_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(References {
        queue_order,
        playlist_items,
    })
}

pub(crate) fn remove_source(conn: &Connection, media_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM audio_queue_items WHERE media_id=?1",
        [media_id],
    )?;
    conn.execute(
        "DELETE FROM audio_playlist_items WHERE media_id=?1",
        [media_id],
    )?;
    Ok(())
}

pub(crate) fn apply(
    conn: &Connection,
    media_id: &str,
    references: References,
) -> rusqlite::Result<()> {
    if let Some(order) = references.queue_order {
        conn.execute("INSERT INTO audio_queue_items(media_id,sort_order) VALUES(?1,?2) ON CONFLICT(media_id) DO UPDATE SET sort_order=excluded.sort_order",params![media_id,order])?;
    }
    for (id, playlist, order, added) in references.playlist_items {
        conn.execute("INSERT INTO audio_playlist_items(id,playlist_id,media_id,sort_order,added_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(playlist_id,media_id) DO UPDATE SET sort_order=min(audio_playlist_items.sort_order,excluded.sort_order),added_at=min(audio_playlist_items.added_at,excluded.added_at)",params![id,playlist,media_id,order,added])?;
    }
    Ok(())
}
