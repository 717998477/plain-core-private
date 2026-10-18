use crate::assembly::server::ContentServer;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn native_main_schema_builds_shared_file_urls_with_typed_variables_without_host_calls() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "fixture").unwrap();
    let server = ContentServer::start(
        directory.path(),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let native = server.native_graphql().unwrap();
    let token = plain_rs::utils::base64::base64_encode_url_safe(&[7; 32]);
    let document = "query($link: SharedLinkInput!, $token: String!) { sharedFileUrl(link: $link, urlToken: $token, virtualPath: \"folder/a.txt\") }";
    let variables = json!({"link":{"host":"127.0.0.1","port":8443,"sharedId":"fixture","token":token,"pageUrl":""},"token":token});
    let response = native
        .execute(
            async_graphql::Request::new(document)
                .variables(async_graphql::Variables::from_json(variables.clone())),
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let value = response.data.into_json().unwrap();
    let url = reqwest::Url::parse(value["sharedFileUrl"].as_str().unwrap()).unwrap();
    assert_eq!(url.path(), "/fs");
    assert_eq!(
        url.query_pairs().find(|(key, _)| key == "sid").unwrap().1,
        "fixture"
    );
    let mut invalid = variables;
    invalid["link"]["port"] = json!(-1);
    let response = native
        .execute(
            async_graphql::Request::new(document)
                .variables(async_graphql::Variables::from_json(invalid)),
        )
        .await;
    assert!(!response.errors.is_empty());
    server.shutdown().await;
}
