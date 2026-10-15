use super::*;

fn row(title: &str) -> MediaFileRow {
    MediaFileRow {
        id: "audio-id".into(),
        data_type: 1,
        path: "/missing/native-song.mp3".into(),
        title: title.into(),
        artist: "Artist".into(),
        bucket_id: "bucket".into(),
        size_bytes: 42,
        duration_ms: 42_000,
        cover_image_id: None,
        width: 0,
        height: 0,
        rotation: 0,
        taken_at: None,
        created_at: "first".into(),
        updated_at: "first".into(),
        favorited: false,
        trashed: false,
        original_path: None,
    }
}

#[test]
fn indexed_audio_preserves_identity_tags_and_duration_and_falls_back_only_for_empty_titles() {
    let metadata = from_row(row("Indexed Title"));
    assert_eq!(metadata.media_id.as_deref(), Some("audio-id"));
    assert_eq!(metadata.track.title, "Indexed Title");
    assert_eq!(metadata.track.artist, "Artist");
    assert_eq!(metadata.track.duration_ms, 42_000);
    let metadata = from_row(row(""));
    assert_eq!(metadata.track.title, "native-song");
    assert_eq!(metadata.track.duration_ms, 42_000);
}

#[cfg(feature = "media")]
#[test]
fn native_unindexed_files_are_probed_for_tags_without_creating_persistent_media_rows() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../plain-rs/testdata/audio-tagged.mp3"
    );
    let metadata = read(&db, path).unwrap();
    assert!(metadata.media_id.is_none());
    assert_eq!(metadata.track.title, "Hydrate Title");
    assert_eq!(metadata.track.artist, "Hydrate Artist");
    assert!(metadata.track.duration_ms > 0);
    assert!(
        db.media_file_by_path(DataType::Audio, path)
            .unwrap()
            .is_none()
    );
    assert!(read(&db, "/missing/nonexistent.mp3").is_err());
}
