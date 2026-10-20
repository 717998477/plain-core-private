use super::*;

fn metadata(path: &str) -> MediaFileMetadata {
    MediaFileMetadata {
        album: String::new(),
        album_artist: String::new(),

        data_type: DataType::Image,
        path: path.into(),
        title: "Photo".into(),
        artist: String::new(),
        bucket_id: "photos".into(),
        size_bytes: 10,
        duration_ms: 0,
        cover_image_id: None,
        width: 640,
        height: 480,
        rotation: 0,
        taken_at: None,
        created_at: "2026-10-12T00:00:00.000Z".into(),
        updated_at: "2026-10-12T00:00:00.000Z".into(),
    }
}

#[test]
fn rescanning_preserves_identity_created_time_and_business_flags() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let mut file = metadata("/photos/one.jpg");
    let original = db.record_media_files(&[file.clone()]).unwrap().remove(0);
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_items SET favorited=1,trashed=1,original_path=path WHERE id=?1",
            [&original.id],
        )
    })
    .unwrap();
    file.title = "Updated".into();
    file.size_bytes = 20;
    file.created_at = "2026-10-13T00:00:00.000Z".into();
    file.updated_at = file.created_at.clone();
    let updated = db.record_media_files(&[file]).unwrap().remove(0);
    assert_eq!(updated.id, original.id);
    assert_eq!(updated.created_at, original.created_at);
    assert_eq!(updated.title, "Updated");
    assert_eq!(updated.size_bytes, 20);
    assert!(updated.favorited && updated.trashed);
    assert_eq!(updated.original_path.as_deref(), Some("/photos/one.jpg"));
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 0);
    assert!(
        db.media_file_list(DataType::Image, 10, 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn batches_rollback_on_invalid_metadata_and_domains_are_separate() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let first = metadata("/photos/one.jpg");
    let mut invalid = metadata("/photos/two.jpg");
    invalid.size_bytes = -1;
    assert!(db.record_media_files(&[first.clone(), invalid]).is_err());
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 0);
    let image = db.record_media_files(&[first.clone()]).unwrap().remove(0);
    let mut audio = first;
    audio.data_type = DataType::Audio;
    let audio = db.record_media_files(&[audio]).unwrap().remove(0);
    assert_ne!(image.id, audio.id);
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 1);
    assert_eq!(db.media_file_count(DataType::Audio).unwrap(), 1);
    assert!(
        db.media_file_by_path(DataType::Image, "/absent")
            .unwrap()
            .is_none()
    );
    assert!(db.media_file_count(DataType::Note).is_err());
}

#[test]
fn unavailable_probe_fields_preserve_current_metadata_but_successful_empty_values_clear_it() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let mut known = metadata("/photos/one.jpg");
    known.artist = "Artist".into();
    known.duration_ms = 12_000;
    known.cover_image_id = Some("album".into());
    known.rotation = 90;
    known.taken_at = Some("2026-10-11T00:00:00.000Z".into());
    let original = db.record_media_files(&[known.clone()]).unwrap().remove(0);
    let mut unknown = metadata(&known.path);
    unknown.title = "Filename fallback".into();
    unknown.artist.clear();
    unknown.duration_ms = 0;
    unknown.cover_image_id = None;
    unknown.width = 0;
    unknown.height = 0;
    unknown.rotation = 0;
    unknown.taken_at = None;
    unknown.size_bytes = 20;
    let preserved = db
        .record_media_file_reads(&[MediaFileMetadataRead {
            metadata: unknown.clone(),
            available: Default::default(),
        }])
        .unwrap()
        .remove(0);
    assert_eq!(preserved.id, original.id);
    assert_eq!(preserved.title, known.title);
    assert_eq!(preserved.artist, known.artist);
    assert_eq!(preserved.duration_ms, known.duration_ms);
    assert_eq!(preserved.cover_image_id, known.cover_image_id);
    assert_eq!(
        (preserved.width, preserved.height, preserved.rotation),
        (known.width, known.height, known.rotation)
    );
    assert_eq!(preserved.taken_at, known.taken_at);
    assert_eq!(preserved.size_bytes, 20);
    let cleared = db
        .record_media_file_reads(&[MediaFileMetadataRead {
            metadata: unknown,
            available: MediaFileMetadataAvailability::ALL,
        }])
        .unwrap()
        .remove(0);
    assert_eq!(cleared.title, "Filename fallback");
    assert!(cleared.artist.is_empty());
    assert_eq!(cleared.duration_ms, 0);
    assert!(cleared.cover_image_id.is_none());
    assert_eq!((cleared.width, cleared.height, cleared.rotation), (0, 0, 0));
    assert_eq!(cleared.taken_at, None);
    assert_eq!(cleared.id, original.id);
}

#[test]
fn availability_aware_batches_are_atomic_and_new_unknown_files_are_not_hidden() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let original = db
        .record_media_files(&[metadata("/photos/one.jpg")])
        .unwrap()
        .remove(0);
    let mut updated = metadata("/photos/one.jpg");
    updated.title = "Replacement".into();
    let mut invalid = metadata("/photos/two.jpg");
    invalid.size_bytes = -1;
    assert!(
        db.record_media_file_reads(&[
            MediaFileMetadataRead {
                metadata: updated,
                available: MediaFileMetadataAvailability::ALL
            },
            MediaFileMetadataRead {
                metadata: invalid,
                available: Default::default()
            },
        ])
        .is_err()
    );
    assert_eq!(
        db.media_file_by_path(DataType::Image, "/photos/one.jpg")
            .unwrap()
            .unwrap(),
        original
    );
    let inserted = db
        .record_media_file_reads(&[MediaFileMetadataRead {
            metadata: metadata("/photos/new.heic"),
            available: Default::default(),
        }])
        .unwrap()
        .remove(0);
    assert_eq!(inserted.path, "/photos/new.heic");
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 2);
}

#[test]
fn forgetting_returns_the_exact_committed_types_and_retains_the_count_contract() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("data.db")).unwrap();
    let image = metadata("/shared");
    let mut audio = image.clone();
    audio.data_type = DataType::Audio;
    db.record_media_files(&[image, audio]).unwrap();
    let mut types = db.forget_media_file_types("/shared").unwrap();
    types.sort_unstable();
    assert_eq!(types, [1, 3]);
    assert_eq!(db.forget_media_file("/shared").unwrap(), 0);
    db.record_media_files(&[metadata("/another")]).unwrap();
    assert_eq!(db.forget_media_file("/another").unwrap(), 1);
}

#[test]
fn forgetting_a_directory_removes_descendants_and_preserves_prefix_neighbors() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("data.db")).unwrap();
    let root = directory.path().join("album%_");
    let nested = root.join("nested/photo.jpg");
    let neighbor = directory.path().join("album%_-other/photo.jpg");
    let row_paths = [root.join("one.jpg"), nested, neighbor.clone()];
    let rows = row_paths
        .iter()
        .map(|path| metadata(path.to_str().unwrap()))
        .collect::<Vec<_>>();
    db.record_media_files(&rows).unwrap();
    assert!(
        db.forget_media_file_types(root.to_str().unwrap())
            .unwrap()
            .is_empty()
    );
    assert!(db.forget_media_path_types("").unwrap().is_empty());
    let types = db.forget_media_path_types(root.to_str().unwrap()).unwrap();
    assert_eq!(types, [3, 3]);
    assert_eq!(db.media_file_count(DataType::Image).unwrap(), 1);
    assert!(
        db.media_file_by_path(DataType::Image, neighbor.to_str().unwrap())
            .unwrap()
            .is_some()
    );
    assert!(
        db.forget_media_path_types(root.to_str().unwrap())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn native_album_grouping_survives_failed_probes_and_clears_removed_album_tags() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("data.db")).unwrap();
    let mut one = metadata("/album/one.mp3");
    one.data_type = DataType::Audio;
    one.album = "Shared Album".into();
    one.album_artist = "Album Artist".into();
    one.artist = "Track One".into();
    let mut two = one.clone();
    two.path = "/album/two.mp3".into();
    two.artist = "Track Two".into();
    let rows = db.record_media_files(&[one.clone(), two]).unwrap();
    assert!(!rows[0].album_id.is_empty());
    assert_eq!(rows[0].album_id, rows[1].album_id);
    let track = crate::media::audio::queue::track_by_id(&db, &rows[0].id).unwrap();
    assert_eq!(track.album_id, rows[0].album_id);
    let mut unreadable = one.clone();
    unreadable.album.clear();
    unreadable.album_artist.clear();
    let retained = db
        .record_media_file_reads(&[MediaFileMetadataRead {
            metadata: unreadable.clone(),
            available: Default::default(),
        }])
        .unwrap()
        .remove(0);
    assert_eq!(retained.album, "Shared Album");
    assert_eq!(retained.album_artist, "Album Artist");
    assert_eq!(retained.album_id, rows[0].album_id);
    let cleared = db.record_media_files(&[unreadable]).unwrap().remove(0);
    assert_eq!(cleared.id, rows[0].id);
    assert!(cleared.album_id.is_empty());
    assert!(
        crate::media::audio::queue::track_by_id(&db, &cleared.id)
            .unwrap()
            .album_id
            .is_empty()
    );
    one.album = "New Album".into();
    let changed = db.record_media_files(&[one]).unwrap().remove(0);
    assert_eq!(changed.id, rows[0].id);
    assert_ne!(changed.album_id, rows[0].album_id);
}
