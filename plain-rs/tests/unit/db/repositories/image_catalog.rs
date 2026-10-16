use super::*;

fn add(connection: &Connection, id: &str, kind: i32, trashed: bool) {
    connection.execute("INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,width,height,rotation,created_at,updated_at,trashed) VALUES(?1,?2,?3,'image/png',?1,?1,'','',1,0,1,1,0,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',?4)", params![id,kind,format!("/catalog/{id}.png"),trashed]).unwrap();
}

#[test]
fn native_pages_are_bounded_keyset_pages_of_live_images_and_resolve_current_ids() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.with_conn(|connection| {
        for n in (0..256).rev() {
            add(connection, &format!("image-{n:04}"), 3, false);
        }
        add(connection, "audio", 1, false);
        add(connection, "trashed", 3, true);
    });
    let catalog = snapshot(&db).unwrap();
    assert_eq!(catalog.total, 256);
    let first = page(&db, &catalog.revision, "", 128).unwrap();
    assert_eq!(first.items.len(), 128);
    assert_eq!(first.items[0].id, "image-0000");
    assert_eq!(first.next_cursor, "image-0127");
    assert!(!first.done);
    let second = page(&db, &catalog.revision, &first.next_cursor, 128).unwrap();
    assert_eq!(second.items.len(), 128);
    assert_eq!(second.items[0].id, "image-0128");
    assert_eq!(second.next_cursor, "image-0255");
    let end = page(&db, &catalog.revision, &second.next_cursor, 128).unwrap();
    assert!(end.done);
    assert!(end.items.is_empty());
    let selected = resolve(
        &db,
        &catalog.revision,
        &[
            "image-0255".into(),
            "trashed".into(),
            "missing".into(),
            "audio".into(),
            "image-0000".into(),
        ],
    )
    .unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|image| image.id.as_str())
            .collect::<Vec<_>>(),
        ["image-0000", "image-0255"]
    );
    assert!(page(&db, &catalog.revision, "", 0).is_err());
    assert!(page(&db, &catalog.revision, "", 129).is_err());
    assert!(resolve(&db, &catalog.revision, &vec!["image-0000".into(); 101]).is_err());
}

#[test]
fn catalog_writes_invalidate_revision_but_embedding_writes_do_not_invalidate_the_scan() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.with_conn(|connection| add(connection, "one", 3, false));
    let original = snapshot(&db).unwrap();
    crate::media::images::embeddings::save(
        &db,
        &[crate::media::images::embeddings::EmbeddingInput {
            id: "one".into(),
            path: "/catalog/one.png".into(),
            embedding_base64: crate::utils::base64::base64_encode(&1.0f32.to_be_bytes()),
        }],
    )
    .unwrap();
    verify(&db, &original.revision).unwrap();
    db.with_conn(|connection| {
        connection.execute(
            "UPDATE media_items SET path='/catalog/changed.png' WHERE id='one'",
            [],
        )
    })
    .unwrap();
    assert!(verify(&db, &original.revision).is_err());
    assert!(page(&db, &original.revision, "", 128).is_err());
    assert!(resolve(&db, &original.revision, &["one".into()]).is_err());
    let current = snapshot(&db).unwrap();
    assert_eq!(
        resolve(&db, &current.revision, &["one".into()]).unwrap()[0].path,
        "/catalog/changed.png"
    );
}

#[test]
fn commits_from_another_connection_invalidate_before_more_rows_or_pruning_are_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("catalog.db");
    let db = Db::open(&path).unwrap();
    db.with_conn(|connection| add(connection, "one", 3, false));
    let original = snapshot(&db).unwrap();
    let external = Connection::open(&path).unwrap();
    add(&external, "two", 3, false);
    assert!(verify(&db, &original.revision).is_err());
    assert!(page(&db, &original.revision, "", 128).is_err());
    let current = snapshot(&db).unwrap();
    assert_eq!(current.total, 2);
    external
        .execute("DELETE FROM media_items WHERE id='one'", [])
        .unwrap();
    assert!(verify(&db, &current.revision).is_err());
    assert_eq!(snapshot(&db).unwrap().total, 1);
}

#[test]
fn keyset_page_uses_the_catalog_order_index_without_a_temporary_sort() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let plan = db.with_conn(|connection| {
        connection.prepare("EXPLAIN QUERY PLAN SELECT id,path FROM media_items WHERE data_type=?1 AND trashed=0 AND id>?2 ORDER BY id LIMIT ?3").unwrap()
            .query_map(params![3,"cursor",128], |row| row.get::<_,String>(3)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    });
    assert!(
        plan.iter()
            .any(|step| step.contains("idx_media_items_data_type_trashed_id")),
        "{plan:?}"
    );
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "{plan:?}"
    );
}

#[test]
fn rollback_noop_and_other_media_updates_do_not_invalidate_an_image_pass() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.with_conn(|connection| {
        add(connection, "image", 3, false);
        add(connection, "audio", 1, false);
    });
    let original = snapshot(&db).unwrap();
    db.with_conn(|connection| {
        connection
            .execute(
                "UPDATE media_items SET path='/audio/changed.mp3' WHERE id='audio'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE media_items SET updated_at=updated_at WHERE id='image'",
                [],
            )
            .unwrap();
        let transaction = connection.unchecked_transaction().unwrap();
        transaction
            .execute("DELETE FROM media_items WHERE id='image'", [])
            .unwrap();
        transaction.rollback().unwrap();
    });
    verify(&db, &original.revision).unwrap();
    assert_eq!(
        page(&db, &original.revision, "", 128).unwrap().items[0].id,
        "image"
    );
}
