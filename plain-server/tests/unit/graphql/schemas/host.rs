use super::*;
#[test]
fn host_settings_schema_matches_committed_sdl() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = std::sync::Arc::new(
        plain_rs::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap(),
    );
    let db = std::sync::Arc::new(plain_rs::db::Db::open(std::path::Path::new(":memory:")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let sdl = build(db, events, prefs, dir.path().into()).sdl();
    assert!(sdl.contains("systemSettings: SystemSettings!"));
    assert!(sdl.contains("userSettings: UserSettings!"));
    assert!(sdl.contains("systemPrefs: JSON!"));
    if std::env::var_os("UPDATE_CONTENT_SCHEMA").is_some() {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../plain-rs/testdata/content-api.graphqls"
            ),
            &sdl,
        )
        .unwrap();
    } else {
        assert_eq!(
            sdl,
            include_str!("../../../../../plain-rs/testdata/content-api.graphqls")
        );
    }
}

#[tokio::test]
async fn duration_mutations_use_fjall_and_restore_file_identity() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = std::sync::Arc::new(
        plain_rs::prefs::Prefs::load(&directory.path().join("system_prefs.json")).unwrap(),
    );
    let db = std::sync::Arc::new(plain_rs::db::Db::open(std::path::Path::new(":memory:")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema = build(db.clone(), events, prefs, directory.path().into());
    let saved = schema.execute(r#"mutation { mediaHostSaveDuration(type: VIDEO, mediaId: "same", durationMs: 5000000001, path: "/file.mp4", sizeBytes: 42, modifiedAt: "2026-01-01T00:00:00.000Z") }"#).await;
    assert!(saved.errors.is_empty(), "{:?}", saved.errors);
    let restored = schema
        .execute("{ mediaHostDurations { type mediaId durationMs path sizeBytes modifiedAt } }")
        .await;
    assert!(restored.errors.is_empty(), "{:?}", restored.errors);
    let restored = restored.data.into_json().unwrap();
    assert_eq!(
        restored["mediaHostDurations"][0]["durationMs"],
        5_000_000_001i64
    );
    assert_eq!(restored["mediaHostDurations"][0]["path"], "/file.mp4");
    assert_eq!(
        restored["mediaHostDurations"][0]["modifiedAt"],
        "2026-01-01T00:00:00.000Z"
    );
    let removed = schema
        .execute(r#"mutation { mediaHostRemoveDurations(type: VIDEO, mediaIds: ["same"]) }"#)
        .await;
    assert!(removed.errors.is_empty(), "{:?}", removed.errors);
    assert!(
        plain_rs::media::duration_cache::DurationCache::new(directory.path())
            .all()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.with_conn(|conn| conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='media_item'",
            [],
            |row| row.get::<_, i64>(0)
        ))
        .unwrap(),
        0
    );
}
