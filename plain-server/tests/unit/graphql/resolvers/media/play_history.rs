use super::*;
use crate::graphql::{executor::RequestOrigin, schemas::public::PublicSchema};
use crate::host::bridge::Host;
use plain_rs::{enums::DataType, prefs::Prefs};
use serde_json::{Value, json};

fn fixture(permissions: &[&str]) -> (tempfile::TempDir, Arc<Db>, PublicSchema) {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set("api_permissions", permissions).unwrap();
    let db = Arc::new(Db::open(&directory.path().join("db")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema = crate::graphql::schemas::public::build(
        Arc::new(Host::default()),
        events,
        prefs,
        db.clone(),
        directory.path().to_owned(),
    );
    (directory, db, schema)
}

fn audio(db: &Db, id: &str, title: &str, artist: &str) {
    db.with_conn(|conn| conn.execute("INSERT INTO media_items(id,data_type,path,mime_type,file_name,title,artist,bucket_id,size_bytes,duration_ms,cover_image_id,width,height,rotation,created_at,updated_at) VALUES(?1,1,?2,'audio/mpeg',?3,?3,?4,'',1,1000,'',0,0,0,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",rusqlite::params![id,format!("/music/{id}.mp3"),title,artist])).unwrap();
    play_history::record(db, DataType::Audio, id, 123).unwrap();
}

async fn query(schema: &PublicSchema, text: &str, offset: i32, limit: i32) -> Value {
    let response = schema.execute(async_graphql::Request::new("query($query: String!, $offset: Int!, $limit: Int!) { mediaPlayHistory(query: $query, offset: $offset, limit: $limit) { mediaId positionMs playCount media { id title path } } }")
        .variables(async_graphql::Variables::from_json(json!({"query":text,"offset":offset,"limit":limit})))
        .data(RequestOrigin::Remote)).await;
    serde_json::to_value(response).unwrap()
}

#[tokio::test]
async fn history_filters_current_metadata_before_paging_and_skips_missing_media() {
    let (_directory, db, schema) = fixture(&[STORAGE]);
    audio(&db, "older", "Road Song", "Ferry");
    audio(&db, "newer", "Another Song", "Ferry");
    audio(&db, "unrelated", "Unrelated", "Other");
    play_history::record(&db, DataType::Audio, "missing", 0).unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_play_history SET played_at='2026-01-01T00:00:00.000Z'",
            [],
        )
    })
    .unwrap();
    let all = query(&schema, "type:AUDIO ferry", 0, 10).await;
    assert!(all["errors"].is_null(), "{all}");
    assert_eq!(
        all["data"]["mediaPlayHistory"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["mediaId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["newer", "older"]
    );
    let page = query(&schema, "type:AUDIO ferry", 1, 1).await;
    assert!(page["errors"].is_null(), "{page}");
    assert_eq!(
        page["data"]["mediaPlayHistory"].as_array().unwrap().len(),
        1
    );
    assert_eq!(page["data"]["mediaPlayHistory"][0]["mediaId"], "older");
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_items SET title='Updated',artist='Changed' WHERE id='older'",
            [],
        )
    })
    .unwrap();
    let updated = query(&schema, "ids:older changed", 0, 10).await;
    assert!(updated["errors"].is_null(), "{updated}");
    let row = &updated["data"]["mediaPlayHistory"][0];
    assert_eq!(row["media"]["title"], "Updated");
    assert_eq!(row["positionMs"], 123);
    assert_eq!(row["playCount"], 1);
    assert_eq!(
        query(&schema, "ferry", 0, 10).await["data"]["mediaPlayHistory"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn history_requires_storage_access_and_rejects_invalid_page_windows() {
    let (_denied_dir, denied_db, denied) = fixture(&[]);
    audio(&denied_db, "private", "Private", "Artist");
    assert!(!query(&denied, "", 0, 10).await["errors"].is_null());
    let (_directory, db, schema) = fixture(&[STORAGE]);
    audio(&db, "one", "One", "Artist");
    for (offset, limit) in [(-1, 10), (0, -1)] {
        assert!(!query(&schema, "", offset, limit).await["errors"].is_null());
    }
    let empty = query(&schema, "", 0, 0).await;
    assert!(empty["errors"].is_null(), "{empty}");
    assert_eq!(empty["data"]["mediaPlayHistory"], json!([]));
}
