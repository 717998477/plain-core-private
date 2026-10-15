use plain_rs::db::{Db, MediaFileRow, audio_catalog::AudioMedia};
use plain_rs::enums::DataType;
use plain_rs::error::{DomainError, DomainResult};
use plain_rs::media::audio::queue::AudioTrack;

pub(crate) fn from_row(row: MediaFileRow) -> AudioMedia {
    AudioMedia {
        media_id: Some(row.id),
        aliases: Vec::new(),
        track: AudioTrack {
            media_id: None,
            path: row.path.clone(),
            title: if row.title.is_empty() {
                AudioTrack::from_path_stem(&row.path).title
            } else {
                row.title
            },
            artist: row.artist,
            duration_ms: row.duration_ms,
            album_id: String::new(),
        },
    }
}

pub(crate) fn read(db: &Db, path: &str) -> DomainResult<AudioMedia> {
    if path.trim().is_empty() {
        return Err(DomainError::Other("Audio path required".into()));
    }
    if let Some(row) = db.media_file_by_path(DataType::Audio, path)? {
        return Ok(from_row(row));
    }
    #[cfg(feature = "media")]
    {
        let facts = plain_media::metadata::file_facts::read_file_metadata_with_availability(
            std::path::Path::new(path),
            DataType::Audio,
        )
        .map_err(|error| DomainError::Other(error.to_string()))?;
        Ok(AudioMedia {
            media_id: None,
            aliases: Vec::new(),
            track: AudioTrack {
                media_id: None,
                path: facts.metadata.path,
                title: facts.metadata.title,
                artist: facts.metadata.artist,
                duration_ms: facts.metadata.duration_ms,
                album_id: String::new(),
            },
        })
    }
    #[cfg(not(feature = "media"))]
    Err(DomainError::Other(
        "Audio metadata reader unavailable".into(),
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/services/audio/metadata.rs"]
mod tests;
