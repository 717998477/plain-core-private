use crate::assembly::server::ContentServer;
use async_graphql::Request;
use serde_json::json;

async fn rename(
    core: &ContentServer,
    path: &std::path::Path,
    name: &str,
) -> async_graphql::Response {
    core.native_graphql()
        .unwrap()
        .execute(
            Request::new(
                "mutation($path:String!,$name:String!){renameFile(path:$path,name:$name)}",
            )
            .variables(async_graphql::Variables::from_json(
                json!({"path":path,"name":name}),
            )),
        )
        .await
}

fn relation(core: &ContentServer, tag: &str) -> Option<String> {
    core.runtime_state()
        .db
        .with_conn(|connection| {
            use rusqlite::OptionalExtension;
            connection
                .query_row(
                    "SELECT target_key FROM tag_relations WHERE tag_id=?1",
                    [tag],
                    |row| row.get(0),
                )
                .optional()
        })
        .unwrap()
}

#[tokio::test]
async fn strict_native_rename_and_delete_rebind_and_clean_generic_file_references_without_host() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(&directory.path().join("core")).unwrap();
    let state = core.runtime_state();
    let (_generation, mut requests) = state.host.connect();
    let source = directory.path().join("source.txt");
    let collision = directory.path().join("existing.txt");
    let destination = directory.path().join("renamed.txt");
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&collision, b"existing").unwrap();
    state.db.with_conn(|connection| connection.execute(
        "INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('file',?1,22,'first')", [source.to_str().unwrap()]
    )).unwrap();
    for name in ["existing.txt", "..", "../escape", ""] {
        let response = rename(&core, &source, name).await;
        assert!(response.errors.is_empty(), "{name}: {:?}", response.errors);
        assert_eq!(response.data.into_json().unwrap()["renameFile"], false);
        assert_eq!(std::fs::read(&source).unwrap(), b"source");
        assert_eq!(std::fs::read(&collision).unwrap(), b"existing");
    }
    let response = rename(&core, &source, "renamed.txt").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["renameFile"], true);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"source");
    assert_eq!(
        relation(&core, "file"),
        Some(destination.to_str().unwrap().into())
    );
    let deleted = core
        .native_graphql()
        .unwrap()
        .execute(
            Request::new("mutation($paths:[String!]!){deleteFiles(paths:$paths){affectedCount}}")
                .variables(async_graphql::Variables::from_json(
                    json!({"paths":[destination]}),
                )),
        )
        .await;
    assert!(deleted.errors.is_empty(), "{:?}", deleted.errors);
    assert_eq!(
        deleted.data.into_json().unwrap()["deleteFiles"]["affectedCount"],
        1
    );
    assert!(!destination.exists());
    assert!(relation(&core, "file").is_none());
    assert!(
        !state
            .files
            .delete(destination.to_str().unwrap().into())
            .await
            .unwrap()
            .removed
    );
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}

#[cfg(feature = "media")]
#[tokio::test]
async fn rename_postprocessing_failure_preserves_physical_receipt_and_does_not_retry_after_restart()
{
    let directory = tempfile::tempdir().unwrap();
    let core_directory = directory.path().join("core");
    let first = ContentServer::open(&core_directory).unwrap();
    let state = first.runtime_state();
    let source = directory.path().join("source.txt");
    let destination = directory.path().join("renamed.txt");
    std::fs::write(&source, b"fixture").unwrap();
    state.db.with_conn(|connection| connection.execute(
        "INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('file',?1,22,'first')", [source.to_str().unwrap()]
    )).unwrap();
    state.media_scans.close().unwrap();
    let response = rename(&first, &source, "renamed.txt").await;
    assert_eq!(response.errors.len(), 1);
    assert!(
        response.errors[0]
            .message
            .contains("rename postprocessing failed")
    );
    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"fixture");
    let owner = state.native_client_id().unwrap();
    let failed = state.files.list(owner.clone()).await.unwrap().remove(0);
    assert_eq!(failed.status, plain_rs::files::tasks::FileTaskStatus::Error);
    assert_eq!(failed.completed_ops[0].dst, destination.to_str().unwrap());
    assert_eq!(
        relation(&first, "file"),
        Some(source.to_str().unwrap().into())
    );
    first.shutdown().await;
    drop(state);
    let second = ContentServer::open(&core_directory).unwrap();
    assert!(
        second
            .runtime_state()
            .files
            .list(owner)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        relation(&second, "file"),
        Some(source.to_str().unwrap().into())
    );
    assert_eq!(std::fs::read(&destination).unwrap(), b"fixture");
    assert!(!directory.path().join("renamed (1).txt").exists());
    second.stop_core().await;
}

#[cfg(feature = "media")]
#[tokio::test]
async fn deletion_scan_failure_is_visible_after_cleanup_and_a_repeat_does_not_retry() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(&directory.path().join("core")).unwrap();
    let state = core.runtime_state();
    let source = directory.path().join("source.txt");
    std::fs::write(&source, b"fixture").unwrap();
    state.db.with_conn(|connection| connection.execute(
        "INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('file',?1,22,'first')", [source.to_str().unwrap()]
    )).unwrap();
    state.media_scans.close().unwrap();
    let first = state.files.delete(source.to_str().unwrap().into()).await;
    assert!(
        first
            .err()
            .unwrap()
            .to_string()
            .contains("Rust core stopped")
    );
    assert!(!source.exists());
    assert!(relation(&core, "file").is_none());
    let repeated = state
        .files
        .delete(source.to_str().unwrap().into())
        .await
        .unwrap();
    assert!(!repeated.removed);
    assert!(repeated.paths.is_empty());
    assert!(
        state
            .files
            .list(state.native_client_id().unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    core.stop_core().await;
}

#[cfg(feature = "media")]
#[tokio::test]
async fn native_rename_and_delete_follow_actual_catalog_identities() {
    use plain_rs::enums::DataType;
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = tempfile::Builder::new()
        .prefix("plain-files-")
        .tempdir_in(home)
        .unwrap();
    let directory_path = directory.path().canonicalize().unwrap();
    let core = ContentServer::open(&directory_path.join("core")).unwrap();
    let state = core.runtime_state();
    let source = directory_path.join("source.png");
    let destination = directory_path.join("renamed.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([20, 40, 60]))
        .save(&source)
        .unwrap();
    let read = plain_media::metadata::file_facts::read_file_metadata_with_availability(
        &source,
        DataType::Image,
    )
    .unwrap();
    let rows = core
        .native_media_catalog()
        .unwrap()
        .record_files(&[read.metadata])
        .unwrap();
    state.db.with_conn(|connection| connection.execute(
        "INSERT INTO tag_relations(tag_id,target_key,type,created_at) VALUES('image',?1,3,'first')", [&rows[0].id]
    )).unwrap();
    let (_generation, mut requests) = state.host.connect();
    let response = rename(&core, &source, "renamed.png").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["renameFile"], true);
    let current = state
        .db
        .media_file_by_path(DataType::Image, destination.to_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(relation(&core, "image"), Some(current.id));
    assert!(
        state
            .db
            .media_file_by_path(DataType::Image, source.to_str().unwrap())
            .unwrap()
            .is_none()
    );
    assert!(
        state
            .files
            .delete(destination.to_str().unwrap().into())
            .await
            .unwrap()
            .removed
    );
    assert!(relation(&core, "image").is_none());
    assert!(!destination.exists());
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}
