use super::*;
use crate::graphql::schemas::public::PublicSchema;
use plain_rs::db::Db;
use serde_json::{Value, json};

fn fixture(permissions: Value) -> (tempfile::TempDir, PublicSchema, Arc<Db>) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("api_permissions", permissions).unwrap();
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    plain_rs::clipboard::record(&db, "hello", "web", "web", false).unwrap();
    let host = Arc::new(Host::default());
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::graphql::schemas::public::build(host, events, prefs, db.clone(), directory),
        db,
    )
}

#[tokio::test]
async fn clipboard_roots_refuse_to_answer_without_the_web_permission() {
    let (_dir, schema, _db) = fixture(json!(["READ_CONTACTS"]));
    for query in [
        r#"query { clipboardItems(offset:0, limit:10, query:"") { id text } }"#,
        r#"query { clipboardItemCount(query:"") }"#,
        r#"mutation { deleteClipboardItems(query:"all:true") { affectedCount } }"#,
        r#"mutation { setClipboard(text:"x") }"#,
    ] {
        let response = schema.execute(query).await;
        assert_eq!(
            response.errors[0].message, "clipboard_sync_disabled",
            "ungated clipboard root: {query}"
        );
    }
}

#[tokio::test]
async fn clipboard_page_window_is_clamped_like_the_contract() {
    let (_dir, schema, _db) = fixture(json!(["CLIPBOARD"]));
    // limit 0 would otherwise ask the database for nothing at all; the
    // contract clamps the window to at least one row.
    let response = schema
        .execute(r#"query { clipboardItems(offset:-5, limit:0, query:"") { id text source label sensitive createdAt } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let items = response.data.into_json().unwrap()["clipboardItems"].clone();
    assert_eq!(items.as_array().unwrap().len(), 1);
    assert_eq!(items[0]["text"], "hello");
    assert_eq!(items[0]["source"], "web");
}

#[tokio::test]
async fn delete_clipboard_items_rejects_a_blank_query() {
    let (_dir, schema, _db) = fixture(json!(["CLIPBOARD"]));
    let response = schema
        .execute(r#"mutation { deleteClipboardItems(query:"   ") { affectedCount } }"#)
        .await;
    assert!(
        response.errors[0]
            .message
            .starts_with("query is required for bulk mutations"),
        "{:?}",
        response.errors
    );

    let response = schema
        .execute(r#"mutation { deleteClipboardItems(query:"all:true") { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteClipboardItems"]["affectedCount"],
        1
    );
}

#[tokio::test]
async fn clipboard_id_selection_deletes_only_selected_rows() {
    let (_dir, schema, db) = fixture(json!(["CLIPBOARD"]));
    let response = schema
        .execute(r#"{ clipboardItems(offset:0, limit:10, query:"") { id } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let id = response.data.into_json().unwrap()["clipboardItems"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    plain_rs::clipboard::record(&db, "unselected", "", "", false).unwrap();
    for selection in [
        "ids:missing".to_owned(),
        "ids:".to_owned(),
        format!("ids:{id},{id},missing"),
    ] {
        let query = format!(
            "mutation {{ deleteClipboardItems(query:{}) {{ affectedCount }} }}",
            serde_json::to_string(&selection).unwrap()
        );
        let response = schema.execute(query).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let expected = i32::from(selection.contains(&id));
        assert_eq!(
            response.data.into_json().unwrap()["deleteClipboardItems"]["affectedCount"],
            expected
        );
        let response = schema.execute(r#"{ clipboardItemCount(query:"") }"#).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(
            response.data.into_json().unwrap()["clipboardItemCount"],
            2 - expected
        );
    }
}
