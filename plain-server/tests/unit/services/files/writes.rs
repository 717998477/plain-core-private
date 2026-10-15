use crate::assembly::server::ContentServer;
use async_graphql::Request;
use serde_json::{Value, json};

async fn graphql(core: &ContentServer, query: &str, variables: Value) -> async_graphql::Response {
    core.native_graphql()
        .unwrap()
        .execute(Request::new(query).variables(async_graphql::Variables::from_json(variables)))
        .await
}

#[tokio::test]
async fn main_writes_preserve_utf8_metadata_and_refuse_overwrite_or_invalid_paths_without_host() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(&directory.path().join("core")).unwrap();
    let state = core.runtime_state();
    let (_generation, mut requests) = state.host.connect();
    let parent = directory.path().join("parent/child");
    let response = graphql(
        &core,
        "mutation($path:String!){createDir(path:$path){isDir}}",
        json!({"path":parent}),
    )
    .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["createDir"]["isDir"],
        true
    );
    let path = parent.join("unicode.txt");
    let content = "fixture 中文\n";
    let query = "mutation($path:String!,$content:String!){writeTextFile(path:$path,content:$content,overwrite:false){name size updatedAt}}";
    let response = graphql(&core, query, json!({"path":path,"content":content})).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["writeTextFile"]["size"], content.len());
    assert_eq!(data["writeTextFile"]["name"], "unicode.txt");
    assert!(
        chrono::DateTime::parse_from_rfc3339(data["writeTextFile"]["updatedAt"].as_str().unwrap())
            .is_ok()
    );
    let response = graphql(&core, query, json!({"path":path,"content":"replacement"})).await;
    assert_eq!(response.errors.len(), 1);
    assert_eq!(std::fs::read_to_string(path).unwrap(), content);
    let response = graphql(
        &core,
        query,
        json!({"path":"relative-file.txt","content":"invalid"}),
    )
    .await;
    assert_eq!(response.errors[0].message, "absolute file path required");
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}

#[cfg(feature = "media")]
#[tokio::test]
async fn a_scan_failure_is_reported_after_the_physical_write_and_does_not_claim_a_result() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(&directory.path().join("core")).unwrap();
    let state = core.runtime_state();
    state.media_scans.close().unwrap();
    let path = directory.path().join("written.txt");
    let response = graphql(&core,
        "mutation($path:String!){writeTextFile(path:$path,content:\"completed\",overwrite:false){name}}", json!({"path":path})).await;
    assert_eq!(response.errors.len(), 1);
    assert!(response.errors[0].message.contains("Rust core stopped"));
    assert_eq!(std::fs::read(&path).unwrap(), b"completed");
    assert!(response.data.into_json().unwrap()["writeTextFile"].is_null());
    core.stop_core().await;
}
