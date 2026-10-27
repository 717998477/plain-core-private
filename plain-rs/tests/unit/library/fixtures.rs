#![allow(dead_code)]

use plain_rs::db::Db;
use plain_rs::error::DomainResult;
use plain_rs::media::audio::queue::AudioTrack;
pub fn test_db(tag: &str) -> Db {
    let dir = tempfile::Builder::new()
        .prefix(&format!("plain-rs-library-{tag}-"))
        .tempdir()
        .unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    plain_rs::test_tempdirs::retain(dir);
    db
}

pub fn audio(path: &str, title: &str, duration_ms: i64) -> AudioTrack {
    AudioTrack {
        media_id: None,
        title: title.to_string(),
        artist: "A".to_string(),
        path: path.to_string(),
        duration_ms,
        album_id: String::new(),
    }
}

pub fn paths_of(items: &[AudioTrack]) -> Vec<String> {
    items.iter().map(|a| a.path.clone()).collect()
}

pub fn register_audio(db: &Db, track: &AudioTrack) {
    if track.path.is_empty() || track.duration_ms < 0 {
        return;
    }
    db.with_conn(|conn| conn.execute("INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,taken_at,created_at,updated_at) VALUES(?1,1,?2,'audio/mpeg',?2,?3,?4,'',1,?5,?6,0,0,0,NULL,'now','now') ON CONFLICT(data_type,path) DO UPDATE SET title=excluded.title,artist=excluded.artist,duration_ms=excluded.duration_ms,cover_image_id=CASE WHEN excluded.cover_image_id<>'' THEN excluded.cover_image_id ELSE media_items.cover_image_id END",rusqlite::params![format!("test:{}",track.path),track.path,track.title,track.artist,track.duration_ms,track.album_id])).unwrap();
}

pub fn register_audio_tracks(db: &Db, tracks: &[AudioTrack]) {
    for track in tracks {
        register_audio(db, track);
    }
}

pub fn indexed_enqueue(db: &Db, tracks: &[AudioTrack], next: bool) -> DomainResult<()> {
    register_audio_tracks(db, tracks);
    plain_rs::media::audio::queue::enqueue(db, tracks, next)
}

pub fn indexed_add_playlist_items(db: &Db, id: &str, tracks: &[AudioTrack]) -> DomainResult<usize> {
    register_audio_tracks(db, tracks);
    plain_rs::media::audio::queue::add_playlist_items(db, id, tracks)
}

pub fn indexed_on_playing(
    db: &Db,
    path: &str,
    title: &str,
    artist: &str,
    duration_ms: i64,
) -> DomainResult<()> {
    register_audio(
        db,
        &AudioTrack {
            media_id: None,
            path: path.into(),
            title: title.into(),
            artist: artist.into(),
            duration_ms,
            album_id: String::new(),
        },
    );
    plain_rs::media::audio::queue::on_playing(db, path)
}

pub fn indexed_current(db: &Db, path: &str) -> DomainResult<()> {
    if !path.is_empty() {
        register_audio(db, &AudioTrack::from_path_stem(path));
    }
    plain_rs::media::audio::queue::save_audio_current(db, path)
}

pub fn history_row(db: &Db, media_id: &str) -> Option<plain_rs::db::MediaPlayHistoryRow> {
    plain_rs::media::play_history::matching(
        db,
        &[plain_rs::utils::search_dsl::FilterField {
            name: "ids".into(),
            op: String::new(),
            value: media_id.into(),
        }],
    )
    .unwrap()
    .into_iter()
    .next()
}
