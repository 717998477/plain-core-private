use super::*;
use crate::media::audio::queue as audio_queue;

#[test]
fn in_memory_databases_have_independent_playback_state() {
    let first = Db::open(std::path::Path::new(":memory:")).unwrap();
    let second = Db::open(std::path::Path::new(":memory:")).unwrap();
    audio_queue::save_audio_current(&first, "one").unwrap();
    seek(&first, 123).unwrap();
    assert_eq!(snapshot(&first.clone()).unwrap().position_ms, 123);
    assert_eq!(snapshot(&second).unwrap(), Playback::default());
}

#[test]
fn recreated_runtime_rejects_reports_from_the_previous_engine_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let db = Db::open(&path).unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    let previous = snapshot(&db).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    let current = invalidate(&db).unwrap();
    assert!(!report(&db, "one", previous.revision, 123).unwrap());
    assert!(report(&db, "one", current.revision, 456).unwrap());
}

#[test]
fn reopening_remembers_only_the_current_track_and_preserves_queue_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let prefs_path = crate::prefs::default_path(dir.path());
    let db = Db::open(&path).unwrap();
    let track = audio_queue::AudioTrack::from_path_stem("one");
    let playlist = audio_queue::create_playlist(&db, "Saved playlist").unwrap();
    audio_queue::add_playlist_items(&db, &playlist.id, std::slice::from_ref(&track)).unwrap();
    let row = prepare_track(&db, &track, true).unwrap();
    loaded(&db, &track.path, row.revision).unwrap();
    assert!(started(&db, &track, row.revision).unwrap());
    seek(&db, 123_456).unwrap();
    let before = std::fs::read(&prefs_path).unwrap();
    let writes = db.with_conn(|c| c.total_changes());
    let playing = invalidate(&db).unwrap();
    assert!(report(&db, "one", playing.revision, 234_567).unwrap());
    loaded(&db, "one", playing.revision).unwrap();
    assert_eq!(db.with_conn(|c| c.total_changes()), writes);
    assert_eq!(std::fs::read(&prefs_path).unwrap(), before);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&before).unwrap(),
        serde_json::json!({"audio_current_path": "one"})
    );
    drop(db);

    let db = Db::open(&path).unwrap();
    let restored = snapshot(&db).unwrap();
    assert_eq!(restored.path, "one");
    assert_eq!(restored.position_ms, 0);
    db.audio().read(|state| {
        assert_eq!(state.load_revision, -1);
        assert_eq!(state.started_revision, -1);
    });
    assert!(!started(&db, &track, playing.revision).unwrap());
    assert_eq!(
        audio_queue::queue_page(&db, &mut audio_queue::NoLibrary, 0, 10, "")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        audio_queue::history_page(&db, 0, 10, "").unwrap()[0].play_count,
        1
    );
    assert_eq!(audio_queue::playlists(&db).unwrap()[0].0.id, playlist.id);
    assert_eq!(
        audio_queue::playlist_item_records_page(&db, &playlist.id, 0, 10, "").unwrap()[0]
            .audio_path,
        "one"
    );
    audio_queue::save_audio_current(&db, "two").unwrap();
    drop(db);
    assert_eq!(snapshot(&Db::open(&path).unwrap()).unwrap().path, "two");
}

#[test]
fn failed_database_commit_restores_current_track_preferences_and_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    let before = seek(&db, 123).unwrap();
    db.with_conn(|c| c.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE parent(id INTEGER PRIMARY KEY); CREATE TABLE child(parent_id INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);")).unwrap();
    assert!(
        db.audio_transaction(|tx, state| {
            state.select("two")?;
            tx.execute("INSERT INTO child VALUES(1)", [])?;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(snapshot(&db).unwrap(), before);
    let prefs = crate::prefs::Prefs::load(&crate::prefs::default_path(dir.path())).unwrap();
    assert_eq!(
        prefs
            .get::<String>(crate::prefs::system::keys::AUDIO_CURRENT_PATH)
            .unwrap()
            .as_deref(),
        Some("one")
    );
}

#[test]
fn failed_preferences_write_keeps_the_source_and_runtime_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let db = Db::open(&path).unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    let before = seek(&db, 123).unwrap();
    let prefs_path = crate::prefs::default_path(dir.path());
    std::fs::remove_file(&prefs_path).unwrap();
    std::fs::create_dir(&prefs_path).unwrap();
    assert!(audio_queue::save_audio_current(&db, "two").is_err());
    assert_eq!(snapshot(&db).unwrap(), before);
    assert_eq!(audio_queue::get_audio_current(&db).unwrap(), "one");
}
#[test]
fn progress_is_runtime_only_and_stale_reports_cannot_overwrite_seek_or_returned_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let db = Db::open(&path).unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    let first = snapshot(&db).unwrap();
    assert!(report(&db, "one", first.revision, 3_000_000_123).unwrap());
    assert_eq!(snapshot(&db.clone()).unwrap().position_ms, 3_000_000_123);
    let resumed = invalidate(&db).unwrap();
    assert_eq!(resumed.position_ms, 3_000_000_123);
    assert!(!report(&db, "one", first.revision, 0).unwrap());
    let sought = seek(&db, 0).unwrap();
    assert!(!report(&db, "one", resumed.revision, 100).unwrap());
    assert!(report(&db, "one", sought.revision, 456).unwrap());
    audio_queue::save_audio_current(&db, "two").unwrap();
    assert_eq!(snapshot(&db).unwrap().position_ms, 0);
    audio_queue::save_audio_current(&db, "one").unwrap();
    assert!(!report(&db, "one", sought.revision, 999).unwrap());
    assert_eq!(snapshot(&db).unwrap().position_ms, 0);
    assert!(seek(&db, -1).is_err());
    assert!(report(&db, "one", 0, -1).is_err());
}
#[test]
fn source_and_progress_reset_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    let mut source = audio_queue::source(&db).unwrap();
    audio_queue::save_source(&db, &source).unwrap();
    seek(&db, 123).unwrap();
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_source BEFORE UPDATE ON audio_queue_source BEGIN SELECT RAISE(ABORT,'fail'); END")).unwrap();
    source.current_path = "two".into();
    source.current_index = 99;
    assert!(audio_queue::save_source(&db, &source).is_err());
    assert_eq!(audio_queue::get_audio_current(&db).unwrap(), "one");
    assert_eq!(snapshot(&db).unwrap().position_ms, 123);
    assert_eq!(seek(&db, 456).unwrap().position_ms, 456);
}

#[test]
fn playback_start_is_once_per_revision_and_failed_history_write_can_retry() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let track = audio_queue::AudioTrack::from_path_stem("one");
    let row = prepare_track(&db, &track, true).unwrap();
    loaded(&db, &track.path, row.revision).unwrap();
    assert!(started(&db, &track, row.revision).unwrap());
    assert!(!started(&db, &track, row.revision).unwrap());
    assert_eq!(
        audio_queue::history_page(&db, 0, 10, "").unwrap()[0].play_count,
        1
    );
    let newer = prepare_track(&db, &track, false).unwrap();
    loaded(&db, &track.path, newer.revision).unwrap();
    assert!(!started(&db, &track, row.revision).unwrap());
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_history BEFORE INSERT ON audio_play_history BEGIN SELECT RAISE(ABORT,'fail'); END")).unwrap();
    assert!(started(&db, &track, newer.revision).is_err());
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_history"))
        .unwrap();
    assert!(started(&db, &track, newer.revision).unwrap());
    assert_eq!(
        audio_queue::history_page(&db, 0, 10, "").unwrap()[0].play_count,
        2
    );
}

#[test]
fn buffered_start_survives_pause_resume_and_seek_but_not_track_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let track = audio_queue::AudioTrack::from_path_stem("one");
    let first = prepare_track(&db, &track, false).unwrap();
    loaded(&db, &track.path, first.revision).unwrap();
    invalidate(&db).unwrap();
    seek(&db, 100).unwrap();
    assert!(started(&db, &track, first.revision).unwrap());
    let next = prepare_track(&db, &track, false).unwrap();
    loaded(&db, &track.path, next.revision).unwrap();
    assert!(!started(&db, &track, first.revision).unwrap());
    assert!(started(&db, &track, next.revision).unwrap());
    assert_eq!(
        audio_queue::history_page(&db, 0, 10, "").unwrap()[0].play_count,
        2
    );
    audio_queue::save_audio_current(&db, "other").unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    assert!(!started(&db, &track, next.revision).unwrap());
}
