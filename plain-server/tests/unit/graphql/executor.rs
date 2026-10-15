use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn local_and_remote_execute_the_same_envelope_and_preserve_errors() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let token = plain_rs::utils::base64::base64_encode(&[8; 32]);
    let server = ContentServer::start(&dir.path().join("db"), &token, prefs).unwrap();
    let state = server.runtime_state();
    let document = json!({"query":"query Count($q: String!) { noteCount(query: $q) } query Other { __typename }", "operationName":"Count", "variables":{"q":""}}).to_string();
    let local = server
        .graphql_executor()
        .execute(serde_json::from_str(&document).unwrap())
        .await;
    let remote = execute_graphql(
        &state.public,
        serde_json::from_str(&document).unwrap(),
        RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert_eq!(
        serde_json::to_value(&local).unwrap(),
        serde_json::to_value(&remote).unwrap()
    );
    assert!(local.errors.is_empty());
    for query in ["{ nonexistentField }", "{ audioHostQueueItems { path } }"] {
        let local = server.graphql_executor().execute(Request::new(query)).await;
        let remote = execute_graphql(
            &state.public,
            Request::new(query),
            RequestContext::remote(Some(state.clone())),
        )
        .await;
        assert!(!local.errors.is_empty());
        assert_eq!(
            serde_json::to_value(local).unwrap(),
            serde_json::to_value(remote).unwrap()
        );
    }
    let query = "{ audioQueuedMediaIds }";
    let local = server.graphql_executor().execute(Request::new(query)).await;
    let remote = execute_graphql(
        &state.public,
        Request::new(query),
        RequestContext::remote(Some(state.clone())),
    )
    .await;
    assert!(local.errors.is_empty(), "{:?}", local.errors);
    assert_eq!(
        serde_json::to_value(local).unwrap(),
        serde_json::to_value(remote).unwrap()
    );
    #[cfg(feature = "http_transport")]
    server.stop_public().await;
    let response = server
        .graphql_executor()
        .execute(Request::new(r#"{ noteCount(query: "") }"#))
        .await;
    assert!(
        response.errors.is_empty(),
        "native execution must survive HTTP shutdown"
    );
    server.shutdown().await;
}

#[test]
fn service_owned_media_notifications_are_not_repeated_by_aliases_or_fragments() {
    for query in [
        "mutation { renamed: mediaAction { affectedCount } }",
        "mutation { ... on Mutation { deleteMediaItems { affectedCount } } }",
        "mutation { ...One ...One } fragment One on Mutation { mediaAction { affectedCount } }",
        "mutation { ...One } fragment One on Mutation { ...Two } fragment Two on Mutation { moveMediaItems { affectedCount } }",
    ] {
        assert!(!content_changes(&Request::new(query)), "{query}");
    }
    assert!(content_changes(&Request::new(
        "mutation { mediaAction { affectedCount } createNote { id } }"
    )));
    assert!(content_changes(&Request::new(
        "mutation { ...One } fragment One on Mutation { createNote { id } }"
    )));
    assert!(!content_changes(&Request::new(
        "mutation { ...One } fragment One on Mutation { ...One }"
    )));
    assert!(!content_changes(
        &Request::new(
            "mutation Media { mediaAction { affectedCount } } mutation Other { createNote { id } }"
        )
        .operation_name("Media")
    ));
}
