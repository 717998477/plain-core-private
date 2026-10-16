use crate::assembly::server::ServerState;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveRequest {
    ids: Vec<String>,
    paths: Vec<String>,
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
        serde_json::to_value(
            rows.into_iter()
                .map(crate::services::audio::metadata::from_row)
                .collect::<Vec<_>>(),
        )
        .map_err(|error| error.to_string())
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
        "audioLibraryResolve" | "systemAudioPlaybackState" | "audioEngineCommand"
    )
}
