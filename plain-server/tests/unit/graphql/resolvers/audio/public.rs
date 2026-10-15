use super::*;
use crate::graphql::schemas::public::PublicSchema;
use crate::host::bridge::Host;
use plain_rs::db::Db;
use plain_rs::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn stub(host: Arc<Host>, db: Arc<Db>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
    let host = host.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            let result = if request["method"] == "audioLibraryResolve"
                || request["method"] == "audioMetadata"
            {
                let params = &request["params"];
                let mut rows = db.with_conn(|conn|conn.prepare("SELECT id,path,title,artist,duration_ms,cover_image_id FROM media_items WHERE data_type=1 UNION SELECT media_id,path,title,artist,duration_ms,album_id FROM platform_audio_media UNION SELECT id,path,title,artist,duration_ms,'' FROM media_items WHERE data_type=1").unwrap().query_map([],|row|Ok(json!({"mediaId":row.get::<_,String>(0)?,"path":row.get::<_,String>(1)?,"title":row.get::<_,String>(2)?,"artist":row.get::<_,String>(3)?,"durationMs":row.get::<_,i64>(4)?,"albumId":row.get::<_,String>(5)?,"aliases":[]}))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap());
                let mut paths = if request["method"] == "audioMetadata" {
                    vec![params["path"].clone()]
                } else {
                    params["paths"].as_array().unwrap().clone()
                };
                if let Some(ids) = params["ids"].as_array() {
                    paths.extend(ids.iter().filter_map(|id| {
                        id.as_str()
                            .and_then(|id| id.strip_prefix("mock:"))
                            .map(|path| json!(path))
                    }));
                }
                for path in &paths {
                    if rows.iter().any(|row| row["path"] == *path) {
                        continue;
                    }
                    let facts = handler("systemAudioPlaylistTracks", json!({"paths":[path]}));
                    for mut track in facts.as_array().unwrap().iter().cloned() {
                        track["mediaId"] =
                            json!(format!("mock:{}", track["path"].as_str().unwrap()));
                        track["albumId"] = json!("");
                        track["aliases"] = json!([]);
                        rows.push(track);
                    }
                }
                if request["method"] == "audioMetadata" {
                    rows.into_iter()
                        .find(|row| row["path"] == params["path"])
                        .unwrap()
                } else {
                    json!(
                        rows.into_iter()
                            .filter(|row| paths.contains(&row["path"])
                                || params["ids"].as_array().unwrap().contains(&row["mediaId"]))
                            .collect::<Vec<_>>()
                    )
                }
            } else if request["method"] == "systemAudioSearchTracks" {
                let mut rows = handler("systemAudioSearchTracks", request["params"].clone());
                if let Some(items) = rows.as_array_mut() {
                    for item in items {
                        item["mediaId"] = json!(format!("mock:{}", item["path"].as_str().unwrap()));
                    }
                }
                rows
            } else if request["method"] == "systemAudioPlaybackPermission" {
                json!(true)
            } else {
                handler(
                    &request["method"].as_str().unwrap_or_default(),
                    request["params"].clone(),
                )
            };
            let _ = host.reply(generation, json!({ "id": id, "result": result }));
        }
    });
}

fn engine_report(params: &Value) -> Value {
    let mut report = params["playback"].clone();
    report["loaded"] = json!(params["action"] == "PLAY");
    report
}

const GRANTED: &str = r#"["WRITE_EXTERNAL_STORAGE"]"#;

fn fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(GRANTED, |method, params| match method {
        "systemAudioPlaylistTracks" => json!(
            params["paths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|path| json!({
                    "title": "Track", "artist": "Artist",
                    "path": path.as_str().unwrap(), "durationMs": 1000,
                }))
                .collect::<Vec<_>>()
        ),
        "systemAudioPlaybackState" => json!({ "isPlaying": false, "positionMs": 0 }),
        "audioEngineCommand" => engine_report(&params),
        "systemAudioSearchTracks" => json!([{
            "title": "Queued", "artist": "Artist", "path": "/music/one.mp3", "durationMs": 1000,
        }]),
        "systemMediaRows" => json!([]),
        "systemMediaCount" => json!(0),
        "systemAudioLyrics" => json!(""),
        // The playback order is the library plus the queue, so the library
        // half has to answer even when nothing is queued from it.
        "audioLibraryCount" => json!(0),
        "audioLibraryPath" => Value::Null,
        "audioLibraryPage" => json!([]),
        "audioLibraryLocate" => json!(-1),
        "audioLibraryContains" => json!(false),
        _other => panic!("unexpected host call {_other}"),
    })
}

fn fixture_with(
    permissions: &str,
    handler: impl Fn(&str, Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs
        .set(
            "api_permissions",
            serde_json::from_str::<Vec<String>>(permissions).unwrap(),
        )
        .unwrap();
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), db.clone(), handler);
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::graphql::schemas::public::build(host, events, prefs, db, directory),
    )
}

fn database(schema: &PublicSchema) -> &Arc<Db> {
    schema.data::<Arc<Db>>().unwrap()
}

async fn query(schema: &PublicSchema, document: &str) -> Value {
    serde_json::to_value(schema.execute(document).await).unwrap()
}

fn track(path: &str, title: &str, artist: &str) -> AudioTrack {
    AudioTrack {
        media_id: None,
        path: path.to_string(),
        title: title.to_string(),
        artist: artist.to_string(),
        album_id: String::new(),
        duration_ms: 1000,
    }
}

fn seed_queue(schema: &PublicSchema, tracks: &[(&str, &str, &str)]) {
    let items: Vec<AudioTrack> = tracks
        .iter()
        .map(|(path, title, artist)| track(path, title, artist))
        .collect();
    for item in &items {
        database(schema).with_conn(|conn|conn.execute("INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,created_at,updated_at) VALUES(?1,1,?2,'audio/mpeg','song',?3,?4,'',1,?5,'',0,0,0,'now','now')",rusqlite::params![format!("test:{}",item.path),item.path,item.title,item.artist,item.duration_ms])).unwrap();
    }
    domain::enqueue(database(schema), &items, false).unwrap();
}

/// A queue seeded behind the resolver's back still reads back through it:
/// that is the point of mounting the app's own service rather than a second
/// copy of the queue.
#[tokio::test]
async fn the_queue_is_paged_and_its_count_matches() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[
            ("/music/a.mp3", "Alpha", "One"),
            ("/music/b.mp3", "Beta", "Two"),
            ("/music/c.mp3", "Gamma", "Three"),
        ],
    );

    let page = query(
        &schema,
        r#"query { audioQueueItems(offset: 0, limit: 2, query: "") { title artist path durationMs } }"#,
    )
    .await;
    let rows = page["data"]["audioQueueItems"]
        .as_array()
        .unwrap_or_else(|| panic!("{page}"));
    assert_eq!(rows.len(), 2, "{page}");
    assert_eq!(rows[0]["durationMs"], 1000);

    let count = query(&schema, r#"query { audioQueueItemCount }"#).await;
    assert_eq!(count["data"]["audioQueueItemCount"], 3);
}

/// `audioPlayback` merges the store and the player, so both halves are
/// asserted: the queue's current path and the platform's own progress.
#[tokio::test]
async fn playback_joins_the_queue_state_and_the_player_state() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemAudioPlaylistTracks" => json!(
            params["paths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|path| json!({
                    "title": "Track", "artist": "Artist",
                    "path": path.as_str().unwrap(), "durationMs": 1000,
                }))
                .collect::<Vec<_>>()
        ),
        "audioEngineCommand" => engine_report(&params),
        "systemAudioPlaybackState" => json!({ "isPlaying": true, "positionMs": 4200 }),
        _other => panic!("unexpected host call {_other}"),
    });
    schema
        .data::<Arc<Prefs>>()
        .unwrap()
        .set_user("audio_play_mode", 2)
        .unwrap();
    query(
        &schema,
        r#"mutation { playAudioByPath(path: "/music/one.mp3") { title path } }"#,
    )
    .await;

    let state = query(
        &schema,
        r#"query { audioPlayback { path mode isPlaying positionMs } }"#,
    )
    .await;
    let playback = &state["data"]["audioPlayback"];
    assert_eq!(playback["path"], "/music/one.mp3", "{state}");
    assert_eq!(playback["mode"], "SHUFFLE");
    assert_eq!(playback["isPlaying"], true);
    assert_eq!(playback["positionMs"], 4200);
}

/// Playing a track must mark it current without touching the queue (2026-09-27,
/// T58). `playAudio` used to append the track to the manual queue, and enqueue
/// moves an already-queued track to the tail — so every desktop play jumped the
/// clicked track to the end. Queueing is the caller's job: they combine
/// `addAudioToQueue`/`enqueue` with the play.
#[tokio::test]
async fn playing_a_track_never_changes_the_queue() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemAudioPlaylistTracks" => json!([{
            "title": "Track", "artist": "Artist",
            "path": params["paths"][0].as_str().unwrap(), "durationMs": 1000,
        }]),
        "audioEngineCommand" => engine_report(&params),
        _other => json!({}),
    });
    database(&schema);
    seed_queue(
        &schema,
        &[
            ("/music/one.mp3", "One", "A"),
            ("/music/two.mp3", "Two", "B"),
        ],
    );

    let played = query(
        &schema,
        r#"mutation { playAudioByPath(path: "/music/one.mp3") { path } }"#,
    )
    .await;
    assert_eq!(
        played["data"]["playAudioByPath"]["path"], "/music/one.mp3",
        "{played}"
    );

    let after = query(
        &schema,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } }"#,
    )
    .await;
    let paths: Vec<&str> = after["data"]["audioQueueItems"]
        .as_array()
        .unwrap_or_else(|| panic!("{after}"))
        .iter()
        .map(|item| item["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec!["/music/one.mp3", "/music/two.mp3"],
        "playAudio must leave the queue exactly as it was — got {paths:?}",
    );
}

/// The store keeps "no current track" as an empty string. The contract says
/// null, and an empty path is not a track a client can act on.
#[tokio::test]
async fn an_idle_player_reports_a_null_current_path() {
    let (_dir, schema) = fixture();
    let state = query(
        &schema,
        r#"query { audioPlayback { path mode isPlaying positionMs } }"#,
    )
    .await;
    assert!(state["errors"].is_null(), "{state}");
    assert!(state["data"]["audioPlayback"]["path"].is_null(), "{state}");
    assert_eq!(state["data"]["audioPlayback"]["mode"], "REPEAT");
}

#[tokio::test]
async fn lyrics_use_embedded_tags_and_missing_tags_are_null() {
    let (dir, schema) = fixture_with(GRANTED, |method, _| panic!("unexpected host call {method}"));
    let track = dir.path().join("a.mp3");
    std::fs::write(&track, []).unwrap();
    let track = track.to_string_lossy().into_owned();
    seed_queue(&schema, &[(&track, "One", "Artist")]);
    let id = format!("test:{track}");
    let none = query(
        &schema,
        &format!(r#"query {{ audioLyrics(mediaId: "{id}") }}"#),
    )
    .await;
    assert!(none["errors"].is_null(), "{none}");
    assert!(none["data"]["audioLyrics"].is_null(), "{none}");

    let (dir, schema) = fixture_with(GRANTED, |method, _| panic!("unexpected host call {method}"));
    let track = dir.path().join("b.mp3");
    let body = b"\x03eng\x00la la la";
    let mut frame = b"USLT".to_vec();
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(body);
    let mut audio = b"ID3\x03\x00\x00".to_vec();
    audio.extend_from_slice(&[0, 0, 0, frame.len() as u8]);
    audio.extend_from_slice(&frame);
    std::fs::write(&track, audio).unwrap();
    std::fs::write(dir.path().join("b.lrc"), "different sidecar").unwrap();
    let track = track.to_string_lossy().into_owned();
    seed_queue(&schema, &[(&track, "Two", "Artist")]);
    let id = format!("test:{track}");
    let some = query(
        &schema,
        &format!(r#"query {{ audioLyrics(mediaId: "{id}") }}"#),
    )
    .await;
    assert_eq!(some["data"]["audioLyrics"], "la la la", "{some}");
}

#[tokio::test]
async fn clearing_empties_the_queue_and_tells_the_player() {
    let (_dir, schema) = fixture();
    seed_queue(&schema, &[("/music/one.mp3", "One", "Artist")]);
    seed_queue(&schema, &[("/music/a.mp3", "Alpha", "One")]);
    let mut events = schema
        .data::<tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>>()
        .unwrap()
        .subscribe();
    let cleared = query(&schema, r#"mutation { clearAudioQueue }"#).await;
    assert_eq!(cleared["data"]["clearAudioQueue"], true, "{cleared}");

    let count = query(&schema, r#"query { audioQueueItemCount }"#).await;
    assert_eq!(count["data"]["audioQueueItemCount"], 0, "{count}");
    assert_eq!(events.try_recv().unwrap().event_type, "CONTENT_CHANGED");
}

#[tokio::test]
async fn reordering_writes_the_order_the_client_sent() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[
            ("/music/a.mp3", "Alpha", "One"),
            ("/music/b.mp3", "Beta", "Two"),
            ("/music/c.mp3", "Gamma", "Three"),
        ],
    );
    let ok = query(
        &schema,
        r#"mutation { reorderAudioQueue(mediaIds: ["test:/music/c.mp3", "test:/music/a.mp3", "test:/music/b.mp3"]) }"#,
    )
    .await;
    assert_eq!(ok["data"]["reorderAudioQueue"], true);

    let items = query(
        &schema,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } }"#,
    )
    .await;
    let paths: Vec<&str> = items["data"]["audioQueueItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec!["/music/c.mp3", "/music/a.mp3", "/music/b.mp3"],
        "{items}"
    );
}

#[tokio::test]
async fn removing_one_item_leaves_the_rest_of_the_queue() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[
            ("/music/a.mp3", "Alpha", "One"),
            ("/music/b.mp3", "Beta", "Two"),
        ],
    );
    let ok = query(
        &schema,
        r#"mutation { removeAudioFromQueue(mediaId: "test:/music/a.mp3") }"#,
    )
    .await;
    assert_eq!(ok["data"]["removeAudioFromQueue"], true);

    let items = query(
        &schema,
        r#"query { audioQueueItems(offset: 0, limit: 10, query: "") { path } }"#,
    )
    .await;
    let paths: Vec<&str> = items["data"]["audioQueueItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["/music/b.mp3"], "{items}");
}

#[tokio::test]
async fn a_playlist_is_created_renamed_and_empties_into_a_count() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[
            ("/music/a.mp3", "Track", "Artist"),
            ("/music/b.mp3", "Track", "Artist"),
        ],
    );
    let db = database(&schema);
    db.with_conn(|c|c.execute("INSERT INTO audio_playlists(id,name,created_at,updated_at) VALUES('seed','Seed','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",[])).unwrap();
    let tracks = domain::queue_page(db, 0, 10, "").unwrap();
    domain::add_playlist_items(db, "seed", &tracks).unwrap();

    let created = query(
        &schema,
        r#"mutation { createAudioPlaylist(name: "Focus") { id name itemCount } }"#,
    )
    .await;
    let playlist = &created["data"]["createAudioPlaylist"];
    let id = playlist["id"].as_str().unwrap().to_string();
    assert_eq!(playlist["name"], "Focus");
    assert_eq!(playlist["itemCount"], 0);

    let added = query(
        &schema,
        &format!(r#"mutation {{ addAudioPlaylistItems(id: "{id}", query: "playlist_id:seed") }}"#),
    )
    .await;
    assert_eq!(added["data"]["addAudioPlaylistItems"], true, "{added}");

    let count = query(
        &schema,
        &format!(r#"query {{ audioPlaylistItemCount(id: "{id}", query: "") }}"#),
    )
    .await;
    assert_eq!(count["data"]["audioPlaylistItemCount"], 2, "{count}");

    let renamed = query(
        &schema,
        &format!(r#"mutation {{ updateAudioPlaylist(id: "{id}", name: "Deep focus") {{ name itemCount }} }}"#),
    )
    .await;
    assert_eq!(renamed["data"]["updateAudioPlaylist"]["name"], "Deep focus");
    // The update reports the row it changed, count included, so a client
    // never has to re-read the playlist to learn its size.
    assert_eq!(renamed["data"]["updateAudioPlaylist"]["itemCount"], 2);

    let listed = query(&schema, r#"query { audioPlaylists { id name itemCount } }"#).await;
    assert_eq!(
        listed["data"]["audioPlaylists"].as_array().map(Vec::len),
        Some(2)
    );

    let _ = query(
        &schema,
        &format!(r#"mutation {{ deleteAudioPlaylist(id: "{id}") }}"#),
    )
    .await;
    let after = query(&schema, r#"query { audioPlaylists { id } }"#).await;
    assert_eq!(
        after["data"]["audioPlaylists"].as_array().map(Vec::len),
        Some(1)
    );
}

#[tokio::test]
async fn removing_a_playlist_item_lowers_its_count() {
    let (_dir, schema) = fixture();
    seed_queue(&schema, &[("/music/a.mp3", "Track", "Artist")]);
    let db = database(&schema);
    db.with_conn(|c|c.execute("INSERT INTO audio_playlists(id,name,created_at,updated_at) VALUES('seed','Seed','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",[])).unwrap();
    let tracks = domain::queue_page(db, 0, 10, "").unwrap();
    domain::add_playlist_items(db, "seed", &tracks).unwrap();

    let created = query(
        &schema,
        r#"mutation { createAudioPlaylist(name: "Mix") { id } }"#,
    )
    .await;
    let id = created["data"]["createAudioPlaylist"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = query(
        &schema,
        &format!(r#"mutation {{ addAudioPlaylistItems(id: "{id}", query: "playlist_id:seed") }}"#),
    )
    .await;

    let removed = query(
        &schema,
        &format!(r#"mutation {{ removeAudioPlaylistItem(id: "{id}", query: "ids:test:/music/a.mp3") {{ affectedCount }} }}"#),
    )
    .await;
    assert_eq!(
        removed["data"]["removeAudioPlaylistItem"]["affectedCount"], 1,
        "{removed}"
    );

    let count = query(
        &schema,
        &format!(r#"query {{ audioPlaylistItemCount(id: "{id}", query: "") }}"#),
    )
    .await;
    assert_eq!(count["data"]["audioPlaylistItemCount"], 0, "{count}");
}

/// An empty playlist has nothing to start, and the contract types the result
/// nullable for exactly that. An error would make "nothing to play" look
/// like a failure.

#[tokio::test]
async fn play_mode_is_written_and_read_from_rust_preferences() {
    let (_dir, schema) = fixture();
    let ok = query(
        &schema,
        r#"mutation { updateAudioPlayMode(mode: REPEAT_ONE) }"#,
    )
    .await;
    assert_eq!(ok["data"]["updateAudioPlayMode"], true, "{ok}");

    let state = query(&schema, r#"query { audioPlayback { mode } }"#).await;
    assert_eq!(state["data"]["audioPlayback"]["mode"], "REPEAT_ONE");
}

/// Without storage the library degrades rather than erroring — the same rule
/// the image and video lists use — while an explicit browse still refuses.
#[tokio::test]
async fn an_ungranted_library_counts_zero_and_refuses_to_list() {
    let (_dir, schema) = fixture_with("[]", |method, _| match method {
        "systemMediaRows" => json!([{
            "id": "1", "title": "Hidden", "artist": "A", "path": "/m/a.mp3",
            "size": 1, "bucketId": "b", "durationMs": 1, "albumFileId": "",
            "createdAt": "2026-01-01T00:00:00.000Z", "updatedAt": "2026-01-01T00:00:00.000Z",
            "isFavorite": false,
        }]),
        _other => panic!("unexpected host call {_other}"),
    });

    let count = query(&schema, r#"query { audioCount(query: "") }"#).await;
    assert_eq!(count["data"]["audioCount"], 0, "{count}");

    let listed = query(
        &schema,
        r#"query { audios(offset: 0, limit: 10, query: "", sortBy: NAME_ASC) { title } }"#,
    )
    .await;
    assert!(
        !listed["errors"].as_array().is_none_or(Vec::is_empty),
        "{listed}"
    );
}

#[tokio::test]
async fn a_library_row_keeps_the_album_art_reference() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _params| match method {
        _other => panic!("unexpected host call {_other}"),
    });
    database(&schema)
        .record_media_files(&[plain_rs::db::MediaFileMetadata {
            data_type: plain_rs::enums::DataType::Audio,
            path: "/m/a.mp3".to_owned(),
            title: "Song".to_owned(),
            artist: "Band".to_owned(),
            bucket_id: "bucket-1".to_owned(),
            size_bytes: 42,
            duration_ms: 1234,
            cover_image_id: Some("album-file-1".to_owned()),
            width: 0,
            height: 0,
            rotation: 0,
            taken_at: None,
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:01.000Z".to_owned(),
        }])
        .unwrap();

    let listed = query(
        &schema,
        r#"query { audios(offset: 0, limit: 10, query: "", sortBy: NAME_ASC) {
            id title artist path durationMs size bucketId albumFileId createdAt updatedAt isFavorite tags { name }
        } }"#,
    )
    .await;
    let row = &listed["data"]["audios"][0];
    assert_eq!(row["albumFileId"], "album-file-1", "{listed}");
    assert_eq!(row["artist"], "Band");
    assert_eq!(row["durationMs"], 1234);
    assert_eq!(row["size"], 42);
    assert_eq!(row["bucketId"], "bucket-1");
    assert_eq!(row["tags"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn playlist_items_expose_catalog_identity_and_path() {
    let (_dir, schema) = fixture();
    let db = database(&schema);
    seed_queue(
        &schema,
        &[
            ("/music/first.mp3", "First", "Artist"),
            ("/music/second.mp3", "Second", "Artist"),
        ],
    );
    let playlist = domain::create_playlist(db, "Original contract").unwrap();
    domain::add_playlist_items(
        db,
        &playlist.id,
        &[
            track("/music/first.mp3", "First", "Artist"),
            track("/music/second.mp3", "Second", "Artist"),
        ],
    )
    .unwrap();
    let response = schema.execute(format!(r#"
        query {{ audioPlaylistItems(id: "{}", query: "", offset: 1, limit: 1) {{ ...OriginalTrack }} }}
        fragment OriginalTrack on AudioItem {{ __typename title artist path durationMs }}
    "#, playlist.id)).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["audioPlaylistItems"],
        json!([
            {"__typename":"AudioItem", "title":"Second", "artist":"Artist", "path":"/music/second.mp3", "durationMs":1000}
        ])
    );
}

#[tokio::test]
async fn adding_tracks_reads_every_search_page_without_an_item_limit() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemAudioPlaylistTracks" => json!(
            params["paths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|path| json!({"path":path,"title":"Track","artist":"Artist","durationMs":123}))
                .collect::<Vec<_>>()
        ),
        other => panic!("unexpected host call {other}"),
    });
    let rows: Vec<_> = (0..1205)
        .map(|i| plain_rs::db::MediaFileMetadata {
            data_type: DataType::Audio,
            path: format!("/track/{i}"),
            title: format!("Track {i}"),
            artist: "Artist".to_owned(),
            bucket_id: "b".to_owned(),
            size_bytes: 10,
            duration_ms: 123,
            cover_image_id: Some(String::new()),
            width: 0,
            height: 0,
            rotation: 0,
            taken_at: None,
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
        })
        .collect();
    database(&schema).record_media_files(&rows).unwrap();

    let added = query(
        &schema,
        r#"mutation { addAudioToQueue(query:"trash:false") }"#,
    )
    .await;
    assert!(added["errors"].is_null(), "{added}");
    let page=query(&schema,r#"{ audioQueueItemCount audioQueueItems(query:"",offset:1200,limit:200) { mediaId path } }"#).await;
    assert_eq!(page["data"]["audioQueueItemCount"], 1205, "{page}");
    let tail = page["data"]["audioQueueItems"].as_array().unwrap();
    assert_eq!(tail.len(), 5, "{page}");
    let paths = tail
        .iter()
        .map(|row| row["path"].as_str().unwrap().to_string())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(paths.len(), 5, "{page}");
}

#[tokio::test]
async fn playing_by_id_leaves_order_and_removing_current_clears_the_engine() {
    let clears = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = clears.clone();
    let (_dir, schema) = fixture_with(GRANTED, move |method, params| match method {
        "audioEngineCommand" => {
            assert_eq!(params["action"], "CLEAR");
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            engine_report(&params)
        }
        "systemAudioPlaybackState" => json!({"isPlaying":false,"positionMs":0}),
        method => panic!("unexpected {method}"),
    });
    seed_queue(
        &schema,
        &[
            ("/one", "One", "Artist"),
            ("/two", "Two", "Artist"),
            ("/three", "Three", "Artist"),
        ],
    );
    let played = query(
        &schema,
        r#"mutation { playAudio(mediaId:"test:/two") { mediaId path } }"#,
    )
    .await;
    assert_eq!(
        played["data"]["playAudio"]["mediaId"], "test:/two",
        "{played}"
    );
    query(
        &schema,
        r#"mutation { reorderAudioQueue(mediaIds:["test:/two","test:/one"]) }"#,
    )
    .await;
    let page = query(
        &schema,
        r#"{ audioQueueItems(query:"",offset:0,limit:10) { mediaId } }"#,
    )
    .await;
    assert_eq!(
        page["data"]["audioQueueItems"],
        json!([{"mediaId":"test:/two"},{"mediaId":"test:/one"},{"mediaId":"test:/three"}])
    );
    let removed = query(
        &schema,
        r#"mutation { removeAudioFromQueue(mediaId:"test:/two") }"#,
    )
    .await;
    assert!(removed["errors"].is_null(), "{removed}");
    let state = query(&schema, "{ audioPlayback { mediaId path } }").await;
    assert!(state["errors"].is_null(), "{state}");
    assert_eq!(
        state["data"]["audioPlayback"]["mediaId"],
        Value::Null,
        "{state}"
    );
    assert_eq!(state["data"]["audioPlayback"]["path"], Value::Null);
    assert_eq!(clears.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn playlist_query_removal_reports_actual_count_without_changing_other_lists() {
    let (_dir, schema) = fixture();
    seed_queue(
        &schema,
        &[("/one", "One", "Artist"), ("/two", "Two", "Artist")],
    );
    let db = database(&schema);
    let first = domain::create_playlist(db, "First").unwrap();
    let second = domain::create_playlist(db, "Second").unwrap();
    let tracks = domain::queue_page(db, 0, 10, "").unwrap();
    domain::add_playlist_items(db, &first.id, &tracks).unwrap();
    domain::add_playlist_items(db, &second.id, &tracks).unwrap();
    let page=query(&schema,&format!(r#"{{ audioPlaylistItems(id:"{}",query:"text:Two",offset:0,limit:10) {{ mediaId }} audioPlaylistItemCount(id:"{}",query:"text:Two") }}"#,first.id,first.id)).await;
    assert_eq!(page["data"]["audioPlaylistItemCount"], 1, "{page}");
    assert_eq!(
        page["data"]["audioPlaylistItems"],
        json!([{"mediaId":"test:/two"}])
    );
    let removed=query(&schema,&format!(r#"mutation {{ removeAudioPlaylistItem(id:"{}",query:"ids:test:/one,test:/absent") {{ affectedCount }} }}"#,first.id)).await;
    assert_eq!(
        removed["data"]["removeAudioPlaylistItem"]["affectedCount"], 1,
        "{removed}"
    );
    assert_eq!(domain::playlist_item_count(db, &second.id).unwrap(), 2);
}
