use rusqlite::Connection;

pub(crate) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media_items (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(id)>0),
    data_type INTEGER NOT NULL CHECK(data_type IN (1,2,3,24)),
    path TEXT NOT NULL CHECK(length(path)>0),
    file_identity TEXT NOT NULL DEFAULT '',
    mime_type TEXT NOT NULL,
    file_name TEXT NOT NULL,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,
    album TEXT NOT NULL DEFAULT '',
    album_artist TEXT NOT NULL DEFAULT '',
    album_id TEXT NOT NULL DEFAULT '',
    bucket_id TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),
    duration_ms INTEGER NOT NULL CHECK(duration_ms>=0),
    cover_image_id TEXT,
    width INTEGER NOT NULL CHECK(width>=0),
    height INTEGER NOT NULL CHECK(height>=0),
    rotation INTEGER NOT NULL CHECK(rotation IN (0,90,180,270)),
    taken_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    favorited INTEGER NOT NULL DEFAULT 0 CHECK(favorited IN (0,1)),
    trashed INTEGER NOT NULL DEFAULT 0 CHECK(trashed IN (0,1)),
    original_path TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS uidx_media_items_data_type_path ON media_items(data_type,path);
CREATE INDEX IF NOT EXISTS idx_media_items_data_type_trashed_updated_at ON media_items(data_type,trashed,updated_at);
CREATE INDEX IF NOT EXISTS idx_media_items_data_type_file_identity ON media_items(data_type,file_identity) WHERE file_identity<>'';
CREATE INDEX IF NOT EXISTS idx_media_items_data_type_trashed_id ON media_items(data_type,trashed,id);",
    )
}
