use super::*;
fn db() -> Db {
    Db::open(std::path::Path::new(":memory:")).unwrap()
}
fn binding(kind: i32, old: &str, new: &str) -> Binding {
    Binding {
        media_type: kind,
        source_id: old.into(),
        destination_id: new.into(),
        source_path: format!("/source/{old}"),
        destination_path: format!("/target/{new}"),
    }
}
#[test]
fn typed_bindings_preserve_tags_video_progress_and_remove_old_audio_paths() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('audio','old',1,'first'),('video','old',2,'first'),('foreign','old',6,'first'),('file','/source/nested/file.txt',22,'first'); INSERT INTO media_play_history(media_type,media_id,position_ms,played_at,play_count) VALUES(2,'old',4000000001,'first',0); INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,created_at,updated_at) VALUES('test:/source/old',1,'/source/old','audio/mpeg','song','audio','','',1,5000000001,'',0,0,0,'now','now'),('test:/unrelated',1,'/unrelated','audio/mpeg','song','unrelated','','',1,1,'',0,0,0,'now','now'); INSERT INTO audio_queue_items(media_id,sort_order) VALUES('test:/source/old',0),('test:/unrelated',1);")).unwrap();
    assert_eq!(
        rebind(
            &db,
            &[
                binding(1, "old", "audio-new"),
                binding(2, "old", "video-new")
            ],
            "/source",
            "/target"
        )
        .unwrap(),
        2
    );
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT position_ms FROM media_play_history WHERE media_id='video-new'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            4000000001
        );
        assert_eq!(
            c.query_row(
                "SELECT target_key FROM tag_relations WHERE tag_id='audio'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "audio-new"
        );
        assert_eq!(
            c.query_row(
                "SELECT target_key FROM tag_relations WHERE tag_id='foreign'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "old"
        );
        assert_eq!(
            c.query_row(
                "SELECT target_key FROM tag_relations WHERE tag_id='file'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "/target/nested/file.txt"
        );
        assert_eq!(
            c.query_row(
                "SELECT m.path FROM audio_queue_items q JOIN audio_media m USING(media_id)",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "/unrelated"
        );
    });
}
#[test]
fn source_snapshots_handle_permuted_provider_ids_without_overwriting_the_other_source() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('one','a',2,'first'),('two','b',2,'second'); INSERT INTO media_play_history(media_type,media_id,position_ms,played_at,play_count) VALUES(2,'a',1,'first',0),(2,'b',2,'second',0);")).unwrap();
    rebind(
        &db,
        &[binding(2, "a", "b"), binding(2, "b", "a")],
        "/source",
        "/target",
    )
    .unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT position_ms FROM media_play_history WHERE media_id='b'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT target_key FROM tag_relations WHERE tag_id='one'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "b"
        );
    });
}
#[test]
fn database_failure_rolls_back_typed_and_path_bindings() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('audio','old',1,'first'),('file','/source/file',22,'first'); INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,created_at,updated_at) VALUES('test:/source/old',1,'/source/old','audio/mpeg','song','audio','','',1,1,'',0,0,0,'now','now'); INSERT INTO audio_queue_items(media_id,sort_order) VALUES('test:/source/old',0); CREATE TRIGGER fail_queue BEFORE DELETE ON audio_queue_items BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(rebind(&db, &[binding(1, "old", "new")], "/source", "/target").is_err());
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM tag_relations WHERE target_key IN ('old','/source/file')",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        )
    });
}
#[test]
fn invalid_bindings_are_rejected_before_any_write_and_path_prefixes_are_exact() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('file','/source-other/file',22,'first');")).unwrap();
    assert!(rebind(&db, &[binding(6, "a", "b")], "/source", "/target").is_err());
    assert!(
        rebind(
            &db,
            &[binding(1, "a", "b"), binding(1, "a", "c")],
            "/source",
            "/target"
        )
        .is_err()
    );
    rebind(&db, &[], "/source", "/target").unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT target_key FROM tag_relations", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "/source-other/file"
        )
    });
}

#[test]
fn private_files_without_provider_ids_clear_only_audio_paths_inside_the_moved_root() {
    let db = db();
    db.with_conn(|c| c.execute_batch("INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,created_at,updated_at) VALUES('test:/private/song',1,'/private/song','audio/mpeg','song','song','','',1,1,'',0,0,0,'now','now'),('test:/private_sibling/song',1,'/private_sibling/song','audio/mpeg','song','other','','',1,1,'',0,0,0,'now','now'); INSERT INTO audio_queue_items(media_id,sort_order) VALUES('test:/private/song',0),('test:/private_sibling/song',1); ")).unwrap();
    crate::media::audio::queue::save_audio_current(&db, "/private/song").unwrap();
    rebind(&db, &[], "/private", "/destination").unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT m.path FROM audio_queue_items q JOIN audio_media m USING(media_id)",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "/private_sibling/song"
        );
    });
    assert_eq!(
        crate::media::audio::queue::get_audio_current(&db).unwrap(),
        ""
    );
}

#[test]
fn raw_alias_and_canonical_roots_rebind_file_tags_and_remove_stale_path_caches() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('raw','/view/old/file',22,'first'),('canonical','/physical/old/file',22,'first'),('sibling','/view/old_sibling/file',22,'first'); INSERT INTO image_embeddings(id,path,embedding,created_at,updated_at) VALUES('raw','/view/old/file',x'3f800000','first','first'); INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,created_at,updated_at) VALUES('test:/view/old/audio',1,'/view/old/audio','audio/mpeg','song','raw','','',1,1,'',0,0,0,'now','now'),('test:/physical/old/audio',1,'/physical/old/audio','audio/mpeg','song','canonical','','',1,1,'',0,0,0,'now','now'),('test:/view/old_sibling/audio',1,'/view/old_sibling/audio','audio/mpeg','song','keep','','',1,1,'',0,0,0,'now','now'); INSERT INTO audio_queue_items(media_id,sort_order) VALUES('test:/view/old/audio',0),('test:/physical/old/audio',1),('test:/view/old_sibling/audio',2);")).unwrap();
    rebind_with_aliases(
        &db,
        &[],
        "/physical/old",
        "/physical/new",
        &[("/view/old".into(), "/view/new".into())],
    )
    .unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT target_key FROM tag_relations WHERE tag_id='raw'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "/view/new/file"
        );
        assert_eq!(
            c.query_row(
                "SELECT target_key FROM tag_relations WHERE tag_id='canonical'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "/physical/new/file"
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM image_embeddings", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM audio_queue_items", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
}
#[test]
fn rebinding_swaps_provider_ids_atomically_and_rolls_back_on_failure() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO media_play_history(media_type,media_id,position_ms,played_at,play_count) VALUES(2,'a',1,'first',0),(2,'b',2,'second',0);")).unwrap();
    let bindings = [binding(2, "a", "b"), binding(2, "b", "a")];
    assert_eq!(rebind(&db, &bindings, "/source", "/target").unwrap(), 2);
    assert_eq!(db.video_progress_get("a").unwrap().unwrap().position_ms, 2);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_move BEFORE DELETE ON media_play_history BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")).unwrap();
    assert!(rebind(&db, &bindings, "/source", "/target").is_err());
    assert_eq!(db.video_progress_get("a").unwrap().unwrap().position_ms, 2);
}
