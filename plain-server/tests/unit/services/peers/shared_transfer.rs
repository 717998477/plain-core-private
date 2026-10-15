use super::*;

#[test]
fn progress_batches_chunks_but_always_reports_the_exact_final_size() {
    let mut progress = Progress::default();
    let reports = (1..=100)
        .filter(|bytes| progress.update(*bytes, 100, Duration::ZERO))
        .collect::<Vec<_>>();
    assert_eq!(reports, [100]);
    let mut progress = Progress::default();
    assert!(!progress.update(1, 10, Duration::from_millis(299)));
    assert!(progress.update(2, 10, Duration::from_millis(300)));
    assert!(!progress.update(3, 10, Duration::from_millis(599)));
    assert!(progress.update(4, 10, Duration::from_millis(600)));
    assert!(progress.update(10, 10, Duration::from_millis(601)));
}
#[test]
fn temporary_guard_removes_partial_file_and_generation_paths_cannot_escape_root() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("partial");
    std::fs::write(&path, b"partial").unwrap();
    drop(Temporary(path.clone()));
    assert!(!path.exists());
    assert!(directory(root.path(), "../outside").is_err());
    let generation = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        directory(root.path(), &generation).unwrap(),
        root.path().join("shared-transfers").join(generation)
    );
}
#[tokio::test]
async fn cleanup_removes_only_the_owned_generation_and_is_repeatable() {
    let root = tempfile::tempdir().unwrap();
    let generation = uuid::Uuid::new_v4().to_string();
    let owned = directory(root.path(), &generation).unwrap();
    tokio::fs::create_dir_all(&owned).await.unwrap();
    tokio::fs::write(owned.join("file"), b"fixture")
        .await
        .unwrap();
    tokio::fs::write(root.path().join("keep"), b"keep")
        .await
        .unwrap();
    cleanup(root.path(), &generation).await.unwrap();
    cleanup(root.path(), &generation).await.unwrap();
    assert!(!owned.exists());
    assert!(root.path().join("keep").exists());
}
