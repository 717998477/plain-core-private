use crate::assembly::server::ServerState;
use plain_rs::db::{MediaFileRow, audio_catalog::AudioMedia};
use plain_rs::enums::DataType;
use plain_rs::media::audio::queue::AudioTrack;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveRequest {
    ids: Vec<String>,
    paths: Vec<String>,
}

fn row(row: MediaFileRow) -> AudioMedia {
    AudioMedia {
        media_id: Some(row.id),
        aliases: Vec::new(),
        track: AudioTrack {
            media_id: None,
            path: row.path,
            title: row.title,
            artist: row.artist,
            duration_ms: row.duration_ms,
            album_id: String::new(),
        },
    }
}

#[derive(Clone)]
pub struct NativeAudio {
    state: ServerState,
}

impl NativeAudio {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub async fn handle(&self, method: &str, params: Value) -> Result<Value, String> {
        self.state.native_client_id()?;
        let service = self.clone();
        let method = method.to_owned();
        tokio::task::spawn_blocking(move || match method.as_str() {
            "audioLibraryResolve" => service.resolve(params),
            "audioMetadata" => service.metadata(params),
            "systemAudioSearchTracks" => service.search(params),
            "systemAudioLyrics" => {
                let path = params["path"].as_str().ok_or("Audio path required")?;
                Ok(Value::String(
                    plain_media::metadata::audio::lyrics::extract_lyrics_from_path(path),
                ))
            }
            "systemAudioPlaybackState" => Ok(json!({"isPlaying":false,"positionMs":0})),
            "audioEngineCommand" => command(params),
            _ => Err("Unsupported audio host method".into()),
        })
        .await
        .map_err(|error| error.to_string())?
    }

    fn resolve(&self, params: Value) -> Result<Value, String> {
        let request: ResolveRequest =
            serde_json::from_value(params).map_err(|error| error.to_string())?;
        let db = self.state.db.clone();
        let rows = db
            .with_conn(|conn| {
                let ids = serde_json::to_string(&request.ids).unwrap();
                let paths = serde_json::to_string(&request.paths).unwrap();
                let sql = format!(
                    "SELECT {} FROM media_items WHERE data_type=1 AND trashed=0 AND (id IN (SELECT value FROM json_each(?1)) OR path IN (SELECT value FROM json_each(?2)))",
                    plain_rs::db::media_file::COLUMNS,
                );
                conn.prepare(&sql)?
                    .query_map(rusqlite::params![ids, paths], plain_rs::db::media_file::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|error| error.to_string())?;
        serde_json::to_value(rows.into_iter().map(row).collect::<Vec<_>>())
            .map_err(|error| error.to_string())
    }

    fn metadata(&self, params: Value) -> Result<Value, String> {
        let path = params["path"]
            .as_str()
            .filter(|path| !path.is_empty())
            .ok_or("Audio path required")?;
        let db = self.state.db.clone();
        let item = if let Some(item) = db
            .media_file_by_path(DataType::Audio, path)
            .map_err(|error| error.to_string())?
        {
            row(item)
        } else {
            let facts = plain_media::metadata::file_facts::read_file_metadata_with_availability(
                std::path::Path::new(path),
                DataType::Audio,
            )
            .map_err(|error| error.to_string())?;
            AudioMedia {
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
            }
        };
        serde_json::to_value(item).map_err(|error| error.to_string())
    }

    fn search(&self, mut params: Value) -> Result<Value, String> {
        params["dataType"] = json!("AUDIO");
        let mut rows = crate::native::media_catalog::NativeMediaCatalog::new(self.state.clone())
            .rows_facts(params)?;
        for row in &mut rows {
            row["mediaId"] = row["id"].clone();
        }
        Ok(Value::Array(rows))
    }
}

fn command(params: Value) -> Result<Value, String> {
    if params["action"] != "CLEAR" {
        return Err("Native audio playback is unavailable".into());
    }
    let mut playback = params["playback"].clone();
    if !playback.is_object() {
        return Err("Audio playback required".into());
    }
    playback["loaded"] = json!(false);
    Ok(playback)
}

pub fn handles(method: &str) -> bool {
    matches!(
        method,
        "audioLibraryResolve"
            | "audioMetadata"
            | "systemAudioSearchTracks"
            | "systemAudioLyrics"
            | "systemAudioPlaybackState"
            | "audioEngineCommand"
    )
}
