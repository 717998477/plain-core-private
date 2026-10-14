use crate::db::Db;
pub use crate::db::models::audio_queue::HISTORY_KEEP;
pub use crate::db::models::audio_queue::PlayHistory;
pub use crate::db::models::audio_queue::Playlist;
pub use crate::db::models::audio_queue::PlaylistItem;
pub use crate::db::models::audio_queue::QueueItem;
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) mod io {
    use super::*;
    pub fn audio_by_path(
        c: &Connection,
        path: &str,
    ) -> rusqlite::Result<Option<crate::media::audio::queue::AudioTrack>> {
        c.query_row("SELECT m.path,m.title,m.artist,m.duration_ms,m.album_id,m.media_id FROM audio_media m WHERE m.media_id IN (SELECT media_id FROM audio_media WHERE path=?1 UNION SELECT media_id FROM platform_audio_aliases WHERE path=?1)",[path],|row|Ok(crate::media::audio::queue::AudioTrack { media_id:Some(row.get(5)?),path:row.get(0)?,title:row.get(1)?,artist:row.get(2)?,duration_ms:row.get(3)?,album_id:row.get(4)? })).optional()
    }

    pub fn save_current(
        c: &Connection,
        path: &str,
        state: &mut crate::media::audio::state::State,
    ) -> rusqlite::Result<()> {
        state.current_media_id = c.query_row("SELECT media_id FROM audio_media WHERE path=?1 UNION SELECT a.media_id FROM platform_audio_aliases a JOIN audio_media m USING(media_id) WHERE a.path=?1", [path], |row|row.get(0)).optional()?;
        Ok(())
    }
    fn queue_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
        Ok(QueueItem {
            media_id: r.get(5)?,
            path: r.get(0)?,
            sort_order: r.get(1)?,
            title: r.get(2)?,
            artist: r.get(3)?,
            duration_ms: r.get(4)?,
        })
    }
    pub fn all_queue_items(c: &Connection) -> rusqlite::Result<Vec<QueueItem>> {
        c.prepare("SELECT m.path,q.sort_order,m.title,m.artist,m.duration_ms,q.media_id FROM audio_queue_items q JOIN audio_media m USING(media_id) ORDER BY q.sort_order ASC,q.media_id ASC")?.query_map([],queue_row)?.collect()
    }
    pub fn queue_item_by_path(c: &Connection, path: &str) -> rusqlite::Result<Option<QueueItem>> {
        c.query_row(
            "SELECT m.path,q.sort_order,m.title,m.artist,m.duration_ms,q.media_id FROM audio_queue_items q JOIN audio_media m USING(media_id) WHERE m.path=?",
            [path],
            queue_row,
        )
        .optional()
    }
    pub fn replace_queue_items(c: &Connection, items: &[QueueItem]) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_queue_items", [])?;
        let mut insert =
            c.prepare("INSERT INTO audio_queue_items(media_id,sort_order) VALUES(?1,?2)")?;
        for (i, item) in items.iter().enumerate() {
            insert.execute(params![item.media_id, i as i64])?;
        }
        Ok(())
    }
    pub fn remove_queue_item(c: &Connection, path: &str) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_queue_items WHERE media_id IN (SELECT media_id FROM audio_media WHERE path=?)", [path])?;
        Ok(())
    }
    fn playlist_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
        Ok(Playlist {
            id: r.get(0)?,
            name: r.get(1)?,
            created_at: r.get(2)?,
            updated_at: r.get(3)?,
        })
    }
    pub fn all_playlists(c: &Connection) -> rusqlite::Result<Vec<Playlist>> {
        c.prepare("SELECT id,name,created_at,updated_at FROM audio_playlists ORDER BY updated_at DESC,rowid DESC")?.query_map([],playlist_row)?.collect()
    }
    pub fn playlist_by_id(c: &Connection, id: &str) -> rusqlite::Result<Option<Playlist>> {
        c.query_row(
            "SELECT id,name,created_at,updated_at FROM audio_playlists WHERE id=?",
            [id],
            playlist_row,
        )
        .optional()
    }
    pub fn insert_playlist(c: &Connection, p: &Playlist) -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO audio_playlists (id,name,created_at,updated_at) VALUES (?1,?2,?3,?4)",
            params![p.id, p.name, p.created_at, p.updated_at],
        )?;
        Ok(())
    }
    pub fn update_playlist(c: &Connection, p: &Playlist) -> rusqlite::Result<()> {
        c.execute(
            "UPDATE audio_playlists SET name=?1,updated_at=?2 WHERE id=?3",
            params![p.name, p.updated_at, p.id],
        )?;
        Ok(())
    }
    pub fn delete_playlist(c: &Connection, id: &str) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_playlists WHERE id=?", [id])?;
        Ok(())
    }
    fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistItem> {
        Ok(PlaylistItem {
            id: r.get(0)?,
            playlist_id: r.get(1)?,
            audio_path: r.get(2)?,
            title: r.get(3)?,
            artist: r.get(4)?,
            duration_ms: r.get(5)?,
            sort_order: r.get(6)?,
            added_at: r.get(7)?,
            album_id: r.get(8)?,
            media_id: r.get(9)?,
        })
    }
    pub fn playlist_items(c: &Connection, id: &str) -> rusqlite::Result<Vec<PlaylistItem>> {
        c.prepare("SELECT p.id,p.playlist_id,m.path,m.title,m.artist,m.duration_ms,p.sort_order,p.added_at,m.album_id,p.media_id FROM audio_playlist_items p JOIN audio_media m USING(media_id) WHERE playlist_id=? ORDER BY sort_order,p.id")?.query_map([id],item_row)?.collect()
    }
    pub fn playlist_count(c: &Connection, id: &str) -> rusqlite::Result<usize> {
        c.query_row(
            "SELECT COUNT(*) FROM audio_playlist_items p JOIN audio_media m USING(media_id) WHERE playlist_id=?",
            [id],
            |r| r.get(0),
        )
    }
    pub fn playlist_position(
        c: &Connection,
        id: &str,
        path: &str,
    ) -> rusqlite::Result<Option<i64>> {
        c.query_row("SELECT (SELECT COUNT(*) FROM audio_playlist_items AS earlier WHERE earlier.playlist_id=item.playlist_id AND earlier.sort_order<item.sort_order) FROM audio_playlist_items AS item WHERE item.playlist_id=?1 AND item.media_id=(SELECT media_id FROM audio_media WHERE path=?2 UNION SELECT a.media_id FROM platform_audio_aliases a JOIN audio_media m USING(media_id) WHERE a.path=?2)",params![id,path],|r|r.get(0)).optional()
    }
    pub fn playlist_items_page(
        c: &Connection,
        id: &str,
        offset: i64,
        limit: i64,
    ) -> rusqlite::Result<Vec<PlaylistItem>> {
        c.prepare("SELECT p.id,p.playlist_id,m.path,m.title,m.artist,m.duration_ms,p.sort_order,p.added_at,m.album_id,p.media_id FROM audio_playlist_items p JOIN audio_media m USING(media_id) WHERE playlist_id=?1 ORDER BY sort_order,p.id LIMIT ?2 OFFSET ?3")?.query_map(params![id,limit,offset],item_row)?.collect()
    }
    pub fn all_playlist_items(c: &Connection) -> rusqlite::Result<Vec<PlaylistItem>> {
        c.prepare("SELECT p.id,p.playlist_id,m.path,m.title,m.artist,m.duration_ms,p.sort_order,p.added_at,m.album_id,p.media_id FROM audio_playlist_items p JOIN audio_media m USING(media_id)")?.query_map([],item_row)?.collect()
    }
    pub fn insert_playlist_item(c: &Connection, p: &PlaylistItem) -> rusqlite::Result<()> {
        c.execute("INSERT INTO audio_playlist_items (id,playlist_id,media_id,sort_order,added_at) SELECT ?1,?2,media_id,?3,?4 FROM audio_media WHERE media_id IN (SELECT media_id FROM audio_media WHERE path=?5 UNION SELECT media_id FROM platform_audio_aliases WHERE path=?5)",params![p.id,p.playlist_id,p.sort_order,p.added_at,p.audio_path])?;
        Ok(())
    }
    pub fn delete_playlist_items(c: &Connection, id: &str) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_playlist_items WHERE playlist_id=?", [id])?;
        Ok(())
    }
    pub fn remove_playlist_item(c: &Connection, id: &str, path: &str) -> rusqlite::Result<()> {
        c.execute(
            "DELETE FROM audio_playlist_items WHERE playlist_id=?1 AND media_id IN (SELECT media_id FROM audio_media WHERE path=?2 UNION SELECT a.media_id FROM platform_audio_aliases a JOIN audio_media m USING(media_id) WHERE a.path=?2)",
            params![id, path],
        )?;
        Ok(())
    }
    pub fn playlist_item_counts(
        c: &Connection,
    ) -> rusqlite::Result<std::collections::HashMap<String, usize>> {
        c.prepare("SELECT playlist_id,COUNT(*) FROM audio_playlist_items p JOIN audio_media m USING(media_id) GROUP BY playlist_id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }
    fn history_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlayHistory> {
        Ok(PlayHistory {
            path: r.get(0)?,
            title: r.get(1)?,
            artist: r.get(2)?,
            duration_ms: r.get(3)?,
            play_count: r.get(4)?,
            played_at: r.get(5)?,
            media_id: r.get(6)?,
        })
    }
    pub fn all_history(c: &Connection) -> rusqlite::Result<Vec<PlayHistory>> {
        c.prepare("SELECT m.path,m.title,m.artist,m.duration_ms,h.play_count,h.played_at,h.media_id FROM media_play_history h JOIN audio_media m USING(media_id) ORDER BY h.played_at DESC,h.rowid DESC")?.query_map([],history_row)?.collect()
    }
    pub fn upsert_history(c: &Connection, p: &PlayHistory) -> rusqlite::Result<()> {
        c.execute("INSERT INTO media_play_history (media_id,position_ms,play_count,played_at) SELECT media_id,0,?2,?3 FROM audio_media WHERE path=?1 ON CONFLICT(media_id) DO UPDATE SET play_count=excluded.play_count,played_at=excluded.played_at",params![p.path,p.play_count,p.played_at])?;
        Ok(())
    }
    pub fn history_by_path(c: &Connection, path: &str) -> rusqlite::Result<Option<PlayHistory>> {
        c.query_row("SELECT m.path,m.title,m.artist,m.duration_ms,h.play_count,h.played_at,h.media_id FROM media_play_history h JOIN audio_media m USING(media_id) WHERE m.path=?",[path],history_row).optional()
    }
    pub fn trim_history(c: &Connection, keep: usize) -> rusqlite::Result<()> {
        c.execute("DELETE FROM media_play_history WHERE media_id IN (SELECT media_id FROM audio_media) AND media_id NOT IN (SELECT h.media_id FROM media_play_history h JOIN audio_media m USING(media_id) ORDER BY h.played_at DESC,h.rowid DESC LIMIT ?1)",[keep as i64])?;
        Ok(())
    }
    pub fn remove_playlist_items_by_paths(
        c: &Connection,
        paths: &[String],
    ) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_playlist_items WHERE media_id IN (SELECT media_id FROM audio_media WHERE path IN (SELECT value FROM json_each(?1)))",[serde_json::to_string(paths).unwrap()])?;
        Ok(())
    }
    pub fn remove_history(c: &Connection, paths: &[String]) -> rusqlite::Result<()> {
        c.execute(
            "DELETE FROM media_play_history WHERE media_id IN (SELECT media_id FROM audio_media WHERE path IN (SELECT value FROM json_each(?1)))",
            [serde_json::to_string(paths).unwrap()],
        )?;
        Ok(())
    }
    pub fn remove_queue_paths(c: &Connection, paths: &[String]) -> rusqlite::Result<()> {
        c.execute(
            "DELETE FROM audio_queue_items WHERE media_id IN (SELECT media_id FROM audio_media WHERE path IN (SELECT value FROM json_each(?1)))",
            [serde_json::to_string(paths).unwrap()],
        )?;
        Ok(())
    }
}

pub fn all_queue_items(db: &Db) -> rusqlite::Result<Vec<QueueItem>> {
    db.with_conn(|c| io::all_queue_items(c))
}
pub fn queue_item_by_path(db: &Db, path: &str) -> rusqlite::Result<Option<QueueItem>> {
    db.with_conn(|c| io::queue_item_by_path(c, path))
}
pub fn replace_queue_items(db: &Db, items: &[QueueItem]) -> rusqlite::Result<()> {
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        io::replace_queue_items(&tx, items)?;
        tx.commit()
    })
}
pub fn remove_queue_item(db: &Db, path: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_queue_item(c, path))
}
pub fn all_playlists(db: &Db) -> rusqlite::Result<Vec<Playlist>> {
    db.with_conn(|c| io::all_playlists(c))
}
pub fn playlist_by_id(db: &Db, id: &str) -> rusqlite::Result<Option<Playlist>> {
    db.with_conn(|c| io::playlist_by_id(c, id))
}
pub fn insert_playlist(db: &Db, p: &Playlist) -> rusqlite::Result<()> {
    db.with_conn(|c| io::insert_playlist(c, p))
}
pub fn update_playlist(db: &Db, p: &Playlist) -> rusqlite::Result<()> {
    db.with_conn(|c| io::update_playlist(c, p))
}
pub fn delete_playlist(db: &Db, id: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::delete_playlist(c, id))
}
pub fn playlist_items(db: &Db, id: &str) -> rusqlite::Result<Vec<PlaylistItem>> {
    db.with_conn(|c| io::playlist_items(c, id))
}
pub fn all_playlist_items(db: &Db) -> rusqlite::Result<Vec<PlaylistItem>> {
    db.with_conn(|c| io::all_playlist_items(c))
}
pub fn insert_playlist_item(db: &Db, p: &PlaylistItem) -> rusqlite::Result<()> {
    db.with_conn(|c| io::insert_playlist_item(c, p))
}
pub fn delete_playlist_items(db: &Db, id: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::delete_playlist_items(c, id))
}
pub fn remove_playlist_item(db: &Db, id: &str, path: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_playlist_item(c, id, path))
}
pub fn playlist_item_counts(db: &Db) -> rusqlite::Result<std::collections::HashMap<String, usize>> {
    db.with_conn(|c| io::playlist_item_counts(c))
}
pub fn all_history(db: &Db) -> rusqlite::Result<Vec<PlayHistory>> {
    db.with_conn(|c| io::all_history(c))
}
pub fn upsert_history(db: &Db, p: &PlayHistory) -> rusqlite::Result<()> {
    db.with_conn(|c| io::upsert_history(c, p))
}
pub fn history_by_path(db: &Db, path: &str) -> rusqlite::Result<Option<PlayHistory>> {
    db.with_conn(|c| io::history_by_path(c, path))
}
pub fn trim_history(db: &Db, keep: usize) -> rusqlite::Result<()> {
    db.with_conn(|c| io::trim_history(c, keep))
}
pub fn remove_playlist_items_by_paths(db: &Db, paths: &[String]) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_playlist_items_by_paths(c, paths))
}
pub fn remove_history(db: &Db, paths: &[String]) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_history(c, paths))
}
pub fn remove_queue_paths(db: &Db, paths: &[String]) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_queue_paths(c, paths))
}
pub fn playlist_count(db: &Db, id: &str) -> rusqlite::Result<usize> {
    db.with_conn(|c| io::playlist_count(c, id))
}
pub fn playlist_position(db: &Db, id: &str, path: &str) -> rusqlite::Result<Option<i64>> {
    db.with_conn(|c| io::playlist_position(c, id, path))
}
pub fn playlist_items_page(
    db: &Db,
    id: &str,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<PlaylistItem>> {
    db.with_conn(|c| io::playlist_items_page(c, id, offset, limit))
}

#[cfg(test)]
#[path = "../../../tests/unit/db/repositories/audio_queue.rs"]
mod tests;
