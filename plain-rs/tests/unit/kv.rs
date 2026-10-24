use super::*;

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("plainapp-{tag}-{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn round_trip_and_prefix_scan() {
    let dir = tmp_dir("db-roundtrip");
    let db = Db::open(&dir).unwrap();
    db.insert("tag:a", "1").unwrap();
    db.insert("tag:b", "2").unwrap();
    db.insert("other:c", "3").unwrap();
    let mut seen = Vec::new();
    for kv in db.scan_prefix("tag:") {
        let (k, v) = kv.unwrap();
        seen.push((
            String::from_utf8_lossy(&k).to_string(),
            String::from_utf8_lossy(&v).to_string(),
        ));
    }
    assert_eq!(
        seen,
        vec![
            ("tag:a".to_string(), "1".to_string()),
            ("tag:b".to_string(), "2".to_string())
        ]
    );
    db.remove("tag:a").unwrap();
    assert_eq!(db.get("tag:a").unwrap(), None);
}

#[test]
fn batch_is_atomic_staging_area() {
    let dir = tmp_dir("db-batch");
    let db = Db::open(&dir).unwrap();
    db.insert("k1", "v1").unwrap();
    let mut batch = db.batch();
    batch.insert("k2", "v2");
    batch.remove("k1");
    db.apply_batch(batch).unwrap();
    assert_eq!(db.get("k1").unwrap(), None);
    assert_eq!(db.get("k2").unwrap().as_deref(), Some(&b"v2"[..]));
}

#[test]
fn opening_the_same_directory_shares_one_engine() {
    let dir = tempfile::tempdir().unwrap();
    let first = Db::open(dir.path()).unwrap();
    let second = Db::open(&dir.path().join(".")).unwrap();
    assert!(Arc::ptr_eq(&first.0, &second.0));
    first.insert("shared", "value").unwrap();
    assert_eq!(
        second.get("shared").unwrap().as_deref(),
        Some(&b"value"[..])
    );
}

#[test]
fn pending_batch_keeps_the_shared_engine_alive() {
    let directory = tempfile::tempdir().unwrap();
    let first = Db::open(directory.path()).unwrap();
    let mut batch = first.batch();
    batch.insert("pending", "value");
    drop(first);
    let second = Db::open(directory.path()).unwrap();
    second.apply_batch(batch).unwrap();
    assert_eq!(
        second.get("pending").unwrap().as_deref(),
        Some(&b"value"[..])
    );
}

#[test]
fn lazy_store_opens_only_when_used_and_returns_its_owned_handle() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("store");
    let store = LazyDb::new(path.clone());
    assert!(!path.exists());
    let first = store.get().unwrap();
    first.insert("owned", "value").unwrap();
    let second = store.get().unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(second.get("owned").unwrap().as_deref(), Some(&b"value"[..]));
}

#[test]
fn backup_is_one_snapshot_across_bounded_batches() {
    let source = tmp_dir("snapshot-source");
    let target = tmp_dir("snapshot-target");
    let db = Db::open(&source).unwrap();
    for index in 0..1030 {
        db.insert(format!("key-{index:04}"), b"before").unwrap();
    }
    db.insert(b"deleted", b"removed").unwrap();
    db.remove(b"deleted").unwrap();
    let calls = std::cell::Cell::new(0usize);
    db.backup_to(&target, || {
        let call = calls.get();
        calls.set(call + 1);
        if call == 1 {
            db.insert(b"key-0000", b"after").unwrap();
            db.remove(b"key-1029").unwrap();
            db.insert(b"new", b"later").unwrap();
        }
        false
    })
    .unwrap();
    let copy = Db::open(&target).unwrap();
    assert_eq!(copy.iter().count(), 1030);
    for entry in copy.iter() {
        assert_eq!(entry.unwrap().1.as_ref(), b"before");
    }
    assert!(copy.get(b"deleted").unwrap().is_none());
    assert!(copy.get(b"new").unwrap().is_none());
    drop(copy);
    drop(db);
    std::fs::remove_dir_all(source).unwrap();
    std::fs::remove_dir_all(target).unwrap();
}
#[test]
fn backup_cancellation_after_a_batch_removes_partial_store_and_preserves_source() {
    let source = tmp_dir("snapshot-cancel-source");
    let target = tmp_dir("snapshot-cancel-target");
    let db = Db::open(&source).unwrap();
    for index in 0..1024 {
        db.insert(format!("key-{index:04}"), b"value").unwrap();
    }
    let calls = std::cell::Cell::new(0usize);
    assert!(
        db.backup_to(&target, || {
            let call = calls.get();
            calls.set(call + 1);
            call > 514
        })
        .is_err()
    );
    assert!(!target.exists());
    assert_eq!(db.iter().count(), 1024);
    assert!(db.backup_to(&source, || false).is_err());
    assert_eq!(db.iter().count(), 1024);
    drop(db);
    std::fs::remove_dir_all(source).unwrap();
}
#[test]
fn injected_write_failure_is_local_to_one_store_and_preserves_existing_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let first = Db::open(&directory.path().join("first")).unwrap();
    let second = Db::open(&directory.path().join("second")).unwrap();
    first.insert("selected", b"before").unwrap();
    first.insert("neighbor", b"retained").unwrap();
    first.flush().unwrap();
    first.set_write_failure_for_test(true);
    assert!(first.insert("selected", b"after").is_err());
    assert!(first.remove("neighbor").is_err());
    assert!(first.flush().is_err());
    let mut batch = first.batch();
    batch.insert("selected", b"after");
    assert!(first.apply_batch(batch).is_err());
    assert_eq!(first.get("selected").unwrap().unwrap().as_ref(), b"before");
    assert_eq!(first.get("neighbor").unwrap().unwrap().as_ref(), b"retained");
    second.insert("independent", b"written").unwrap();
    second.flush().unwrap();
    first.set_write_failure_for_test(false);
    first.insert("selected", b"after").unwrap();
    first.flush().unwrap();
    assert_eq!(first.get("selected").unwrap().unwrap().as_ref(), b"after");
}
