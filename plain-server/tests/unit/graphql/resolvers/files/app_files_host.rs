use crate::assembly::server::ContentServer;
use crate::test_support::graphql_http as call;
use std::sync::Arc;

#[tokio::test]
async fn file_import_dedupe_release_and_restart_use_core_storage() {
    let dir = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[14; 32]);
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let path = dir.path().join("plain.db");
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let source = dir.path().join("upload");
    std::fs::write(&source, b"shared attachment").unwrap();
    let query = format!(
        r#"mutation {{importAppFile(source:{},fileName:"original.%_",mimeType:"text/plain",deleteSource:false) {{id realPath createdAt size}}}}"#,
        serde_json::to_string(source.to_str().unwrap()).unwrap()
    );
    let first = call(server.port, &token, &query).await;
    assert!(first.get("errors").is_none(), "{first}");
    let file = &first["data"]["importAppFile"];
    let id = file["id"].as_str().unwrap();
    let relative = file["realPath"].as_str().unwrap();
    assert_eq!(file["size"], 17);
    let second = call(server.port, &token, &query).await;
    assert_eq!(
        second["data"]["importAppFile"]["id"], id,
        "an identical import must reuse the stored row"
    );
    let page = call(
        server.port,
        &token,
        r#"{appFiles(offset:0,limit:20,query:"%_") {id} appFileCount(query:"%_")}"#,
    )
    .await;
    assert_eq!(page["data"]["appFileCount"], 1);
    assert_eq!(page["data"]["appFiles"][0]["id"], id);
    let url_token = plain_rs::prefs::ensure_url_token(&prefs);
    let encoded = plain_rs::utils::base64::base64_encode(
        &plain_rs::crypto::xchacha_encrypt(
            &url_token,
            dir.path().join(relative).to_str().unwrap().as_bytes(),
        )
        .unwrap(),
    );
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/fs", server.port);
    let response = client
        .get(&url)
        .query(&[("id", &encoded)])
        .bearer_auth(&token)
        .header("range", "bytes=2-7")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["content-range"], "bytes 2-7/17");
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"ared a");
    let response = client
        .get(&url)
        .query(&[("id", &encoded)])
        .bearer_auth(&token)
        .header("range", "bytes=100-")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 416);
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    let release = format!(r#"mutation {{releaseAppFile(fileId:"{id}")}}"#);
    assert_eq!(
        call(server.port, &token, &release).await["data"]["releaseAppFile"],
        true
    );
    assert!(dir.path().join(relative).exists());
    assert_eq!(
        call(server.port, &token, &release).await["data"]["releaseAppFile"],
        true
    );
    assert!(!dir.path().join(relative).exists());
    assert_eq!(
        call(server.port, &token, &release).await["data"]["releaseAppFile"],
        false
    );
    assert!(source.exists());
    let moved = call(
        server.port,
        &token,
        &query.replace("deleteSource:false", "deleteSource:true"),
    )
    .await;
    assert!(moved.get("errors").is_none(), "{moved}");
    assert!(!source.exists());
    assert!(dir.path().join(relative).exists());
    server.shutdown().await;
}
