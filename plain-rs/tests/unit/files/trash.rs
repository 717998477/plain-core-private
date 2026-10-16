use super::*;

fn item(directory: &Path) -> TrashItem {
    TrashItem {
        id: "owned".into(),
        kind: "file".into(),
        original_path: directory.join("report.txt").to_string_lossy().into_owned(),
        disk: directory.to_string_lossy().into_owned(),
        trash_rel_path: "data/2026/01/f_owned_report.txt".into(),
        deleted_at: 1_700_000_000,
        uid: 0,
        gid: 0,
        mode: 0o600,
        size: Some(7),
        entry_count: None,
    }
}

fn store(directory: &Path) -> Trash {
    Trash::new(Arc::new(crate::kv::Db::open(directory).unwrap()))
}

#[test]
fn bucket_path_layout() {
    let rel = compute_bucket_rel_path(
        "file",
        "abcd",
        "hello world.mp4",
        Utc.with_ymd_and_hms(2026, 3, 15, 0, 0, 0).unwrap(),
    );
    assert!(rel.starts_with("data/2026/03/f_abcd_"));
}

#[test]
fn metadata_and_indexes_use_only_the_supplied_store() {
    let directory = tempfile::tempdir().unwrap();
    let left = store(&directory.path().join("left"));
    let right = store(&directory.path().join("right"));
    let row = item(directory.path());
    left.store_item(&row).unwrap();
    assert_eq!(
        left.load_item("owned").unwrap().unwrap().original_path,
        row.original_path
    );
    assert!(right.load_item("owned").unwrap().is_none());
    assert_eq!(left.trash_count().unwrap(), 1);
    assert_eq!(right.trash_count().unwrap(), 0);
    assert_eq!(
        left.list_trash(0, 10, "report", SortOrder::NameAsc)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(left.db.iter().count(), 5);
    left.delete_item_keys(&row).unwrap();
    assert_eq!(left.db.iter().count(), 0);
}

#[tokio::test]
async fn restore_preserves_an_existing_destination_and_removes_owned_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let trash = store(&directory.path().join("store"));
    let row = item(directory.path());
    let source = abs_trash_path(&row.disk, &row.trash_rel_path);
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"restore").unwrap();
    std::fs::write(&row.original_path, b"existing").unwrap();
    trash.store_item(&row).unwrap();
    let restored = trash.restore_paths(vec!["owned".into()]).await.unwrap();
    assert_eq!(
        restored,
        vec![
            directory
                .path()
                .join("report (restored).txt")
                .to_string_lossy()
        ]
    );
    assert_eq!(std::fs::read(&restored[0]).unwrap(), b"restore");
    assert_eq!(std::fs::read(&row.original_path).unwrap(), b"existing");
    assert!(!source.exists());
    assert_eq!(trash.trash_count().unwrap(), 0);
}

#[tokio::test]
async fn permanent_delete_removes_file_and_all_owned_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let trash = store(&directory.path().join("store"));
    let row = item(directory.path());
    let source = abs_trash_path(&row.disk, &row.trash_rel_path);
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"deleted").unwrap();
    trash.store_item(&row).unwrap();
    trash
        .delete_trash_by_path(source.to_str().unwrap())
        .await
        .unwrap();
    assert!(!source.exists());
    assert_eq!(trash.db.iter().count(), 0);
}

#[tokio::test]
async fn a_physical_delete_error_preserves_all_owned_trash_indexes() {
    let directory = tempfile::tempdir().unwrap();
    let trash = store(&directory.path().join("store"));
    let blocked = directory.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    let mut row = item(&blocked);
    row.disk = blocked.to_string_lossy().into_owned();
    trash.store_item(&row).unwrap();
    std::fs::remove_dir(&blocked).unwrap();
    std::fs::write(&blocked, b"not a directory").unwrap();
    let before = trash.db.iter().count();
    let path = abs_trash_path(&row.disk, &row.trash_rel_path);
    assert!(
        trash
            .delete_trash_by_path(path.to_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(trash.db.iter().count(), before);
    assert!(trash.load_item(&row.id).unwrap().is_some());
    assert_eq!(std::fs::read(blocked).unwrap(), b"not a directory");
}

#[test]
fn inaccessible_volume_trash_uses_the_selected_files_parent_without_changing_filesystems() {
    for kind in [
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::ReadOnlyFilesystem,
        std::io::ErrorKind::NotFound,
    ] {
        let mut attempted = Vec::new();
        let selected = writable_disk(Path::new("/volume/owned/report.txt"), "/volume", |path| {
            attempted.push(path.to_owned());
            if path == "/volume" {
                Err(std::io::Error::from(kind))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempted, ["/volume", "/volume/owned"]);
        assert_eq!(selected, "/volume/owned");
        assert_eq!(
            abs_trash_path(&selected, "data/file"),
            Path::new("/volume/owned/.plain-trash/data/file")
        );
    }
}

#[test]
fn volume_trash_failures_other_than_access_do_not_silently_select_another_root() {
    let mut calls = 0;
    let result = writable_disk(Path::new("/volume/owned/report.txt"), "/volume", |_| {
        calls += 1;
        Err(std::io::Error::from(std::io::ErrorKind::StorageFull))
    });
    assert!(result.is_err());
    assert_eq!(calls, 1);
}

#[cfg(unix)]
#[test]
fn a_symlink_cannot_replace_an_owned_trash_root() {
    let directory = tempfile::tempdir().unwrap();
    let other = directory.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::os::unix::fs::symlink(&other, directory.path().join(".plain-trash")).unwrap();
    assert_eq!(
        prepare_trash_parent(directory.path().to_str().unwrap(), "data/file")
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    assert!(!other.join("data").exists());
}
