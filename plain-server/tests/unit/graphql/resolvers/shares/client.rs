use super::*;
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

#[test]
fn shared_card_input_preserves_large_sizes_expiry_and_peer_identity() {
    let input = SharedMessageCardInput {
        share_id: "share".into(),
        url_token: "token".into(),
        name: "Folder".into(),
        peer_info: SharedPeerAddressInput {
            id: "peer".into(),
            ip: "::1".into(),
            port: 8443,
        },
        item_count: Long(2),
        total_size: Long(9_000_000_000),
        expires_at: Some(Instant(
            chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00.123Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        )),
    };
    let card: share_card::Card = domain(input).unwrap();
    assert_eq!(card.peer_info.port, 8443);
    assert_eq!(card.total_size, 9_000_000_000);
    assert_eq!(card.expires_at.unwrap().timestamp_subsec_millis(), 123);
}

#[test]
fn shared_info_converts_guest_epoch_millis_to_main_instant_without_precision_loss() {
    for value in [None, Some(0), Some(1_893_456_000_123)] {
        let info = client::Info {
            name: "Share".into(),
            read_only: true,
            requires_password: false,
            expires_at: value,
            url_token: "resource".into(),
            entries: vec![],
        };
        let result = SharedLinkInfo::try_from(info.clone()).unwrap();
        assert_eq!(result.expires_at.map(|at| at.0.timestamp_millis()), value);
        let mut invalid = info;
        invalid.expires_at = Some(i64::MAX);
        assert!(SharedLinkInfo::try_from(invalid).is_err());
    }
}
