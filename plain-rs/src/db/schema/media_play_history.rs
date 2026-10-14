use rusqlite::Connection;

pub(crate) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media_play_history (
    media_id TEXT NOT NULL PRIMARY KEY CHECK(length(media_id)>0),
    position_ms INTEGER NOT NULL CHECK(position_ms>=0),
    play_count INTEGER NOT NULL CHECK(play_count>=0),
    played_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_media_play_history_played_at ON media_play_history(played_at);",
    )
}
