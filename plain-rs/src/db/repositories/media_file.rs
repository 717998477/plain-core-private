use crate::db::{
    Db, MediaFileMetadata, MediaFileMetadataAvailability, MediaFileMetadataRead, MediaFileRow,
};
use crate::enums::DataType;
use rusqlite::{OptionalExtension, params};

pub const COLUMNS: &str = "id,data_type,path,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,taken_at,created_at,updated_at,favorited,trashed,original_path,album,album_artist,album_id";

pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaFileRow> {
    Ok(MediaFileRow {
        id: row.get(0)?,
        data_type: row.get(1)?,
        path: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        bucket_id: row.get(5)?,
        size_bytes: row.get(6)?,
        duration_ms: row.get(7)?,
        cover_image_id: row.get(8)?,
        width: row.get(9)?,
        height: row.get(10)?,
        rotation: row.get(11)?,
        taken_at: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        favorited: row.get(15)?,
        trashed: row.get(16)?,
        original_path: row.get(17)?,
        album: row.get(18)?,
        album_artist: row.get(19)?,
        album_id: row.get(20)?,
    })
}

fn kind(data_type: DataType) -> rusqlite::Result<i32> {
    data_type
        .media_type_str()
        .ok_or(rusqlite::Error::InvalidQuery)?;
    Ok(data_type.kind())
}

impl Db {
    pub fn record_media_files(
        &self,
        files: &[MediaFileMetadata],
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        self.record_media_file_inputs(
            files
                .iter()
                .map(|file| (file, MediaFileMetadataAvailability::ALL)),
        )
    }

    pub fn record_media_file_reads(
        &self,
        files: &[MediaFileMetadataRead],
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        self.record_media_file_inputs(files.iter().map(|file| (&file.metadata, file.available)))
    }

    fn record_media_file_inputs<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a MediaFileMetadata, MediaFileMetadataAvailability)>,
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut rows = Vec::new();
            for (file, available) in files {
                let data_type = kind(file.data_type)?;
                tx.execute(
                    "INSERT INTO media_items(id,data_type,path,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,taken_at,created_at,updated_at,mime_type,file_name,album,album_artist,album_id)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?24,?25,?26)
                     ON CONFLICT(data_type,path) DO UPDATE SET album=CASE WHEN ?18 THEN excluded.album ELSE media_items.album END,album_artist=CASE WHEN ?18 THEN excluded.album_artist ELSE media_items.album_artist END,album_id=CASE WHEN ?18 THEN excluded.album_id ELSE media_items.album_id END,title=CASE WHEN ?18 THEN excluded.title ELSE media_items.title END,artist=CASE WHEN ?18 THEN excluded.artist ELSE media_items.artist END,bucket_id=excluded.bucket_id,size_bytes=excluded.size_bytes,duration_ms=CASE WHEN ?19 THEN excluded.duration_ms ELSE media_items.duration_ms END,cover_image_id=CASE WHEN ?20 THEN excluded.cover_image_id ELSE media_items.cover_image_id END,width=CASE WHEN ?21 THEN excluded.width ELSE media_items.width END,height=CASE WHEN ?21 THEN excluded.height ELSE media_items.height END,rotation=CASE WHEN ?22 THEN excluded.rotation ELSE media_items.rotation END,taken_at=CASE WHEN ?23 THEN excluded.taken_at ELSE media_items.taken_at END,updated_at=excluded.updated_at,mime_type=excluded.mime_type,file_name=excluded.file_name",
                    params![uuid::Uuid::new_v4().to_string(),data_type,file.path,file.title,file.artist,file.bucket_id,file.size_bytes,file.duration_ms,file.cover_image_id,file.width,file.height,file.rotation,file.taken_at.as_deref().map(crate::utils::dbtime::normalize).transpose()?,crate::utils::dbtime::normalize(&file.created_at)?,crate::utils::dbtime::normalize(&file.updated_at)?,crate::utils::mime::mime_from_ext(&file.path),std::path::Path::new(&file.path).file_name().and_then(|name|name.to_str()).unwrap_or(&file.path),available.tags,available.duration,available.cover_image_id,available.dimensions,available.rotation,available.taken_at,file.album,file.album_artist,crate::media::audio::metadata::album_id(&file.album,&file.album_artist,&file.artist)],
                )?;
                rows.push(tx.query_row(&format!("SELECT {COLUMNS} FROM media_items WHERE data_type=?1 AND path=?2"),params![data_type,file.path],from_row)?);
            }
            tx.commit()?;
            Ok(rows)
        })
    }

    pub fn media_file_by_path(
        &self,
        data_type: DataType,
        path: &str,
    ) -> rusqlite::Result<Option<MediaFileRow>> {
        let data_type = kind(data_type)?;
        self.with_conn(|conn| {
            conn.query_row(
                &format!("SELECT {COLUMNS} FROM media_items WHERE data_type=?1 AND path=?2"),
                params![data_type, path],
                from_row,
            )
            .optional()
        })
    }

    /// Forget one catalogued path and everything keyed to it. A filesystem
    /// watcher reports the file is gone; without this the row and its
    /// tag, duration and embedding rows would outlive it.
    pub fn forget_media_file(&self, path: &str) -> rusqlite::Result<usize> {
        self.forget_media_file_types(path).map(|types| types.len())
    }

    pub fn forget_media_file_types(&self, path: &str) -> rusqlite::Result<Vec<i32>> {
        self.forget_media_path_rows(path, false)
    }

    pub fn forget_media_path_types(&self, path: &str) -> rusqlite::Result<Vec<i32>> {
        self.forget_media_path_rows(path, true)
    }

    fn forget_media_path_rows(&self, path: &str, recursive: bool) -> rusqlite::Result<Vec<i32>> {
        if path.is_empty() {
            return Ok(Vec::new());
        }
        let prefix = format!(
            "{}{separator}",
            path.trim_end_matches(std::path::MAIN_SEPARATOR),
            separator = std::path::MAIN_SEPARATOR
        );
        self.audio_transaction(|tx, state| {
            let rows = tx
                .prepare("SELECT data_type,id FROM media_items WHERE path=?1 OR (?2 AND substr(path,1,length(?3))=?3)")?
                .query_map(params![path, recursive, prefix], |row| {
                    Ok((row.get::<_, i32>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (data_type, id) in &rows {
                tx.execute(
                    "DELETE FROM tag_relations WHERE type=?1 AND target_key=?2",
                    params![data_type, id],
                )?;
                tx.execute("DELETE FROM image_embeddings WHERE id=?1", [id])?;
                tx.execute(
                    "DELETE FROM media_play_history WHERE media_id=?1 AND media_type=?2",
                    params![id, data_type],
                )?;
                if *data_type == 1 {
                    for table in ["audio_queue_items", "audio_playlist_items"] {
                        tx.execute(&format!("DELETE FROM {table} WHERE media_id=?1"), [id])?;
                    }
                    if state.current_media_id.as_deref() == Some(id.as_str()) {
                        state.select("")?;
                    }
                }
            }
            tx.execute("DELETE FROM media_items WHERE path=?1 OR (?2 AND substr(path,1,length(?3))=?3)", params![path, recursive, prefix])?;
            Ok(rows.into_iter().map(|(kind, _)| kind).collect())
        })
        .map_err(|error| match error {
            crate::error::DomainError::Sqlite(error) => error,
            error => rusqlite::Error::ToSqlConversionFailure(Box::new(error)),
        })
    }

    pub fn media_file_list(
        &self,
        data_type: DataType,
        limit: u32,
        offset: u32,
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        let data_type = kind(data_type)?;
        self.with_conn(|conn| {
            let mut statement = conn.prepare(&format!("SELECT {COLUMNS} FROM media_items WHERE data_type=?1 AND trashed=0 ORDER BY updated_at DESC,id LIMIT ?2 OFFSET ?3"))?;
            statement.query_map(params![data_type,limit,offset],from_row)?.collect()
        })
    }

    pub fn media_file_count(&self, data_type: DataType) -> rusqlite::Result<i64> {
        let data_type = kind(data_type)?;
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM media_items WHERE data_type=?1 AND trashed=0",
                [data_type],
                |row| row.get(0),
            )
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/db/repositories/media_file.rs"]
mod tests;
