use crate::assembly::server::ContentServer;
use crate::graphql::executor::{RequestContext, execute_graphql};

#[tokio::test]
async fn runtime_controls_require_local_authority_and_do_not_emit_content_updates() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let (_generation, mut requests) = state.host.connect();
    let mut events = state.events.subscribe();
    for document in [
        "mutation{controlPeerStatus(action:STOP)}",
        r#"mutation{confirmWebLogin(requestId:"missing-request",allowed:true)}"#,
    ] {
        let remote = execute_graphql(
            &state.public,
            async_graphql::Request::new(document),
            RequestContext::remote(Some(state.clone())),
        )
        .await;
        assert_eq!(remote.errors.len(), 1);
        assert_eq!(remote.errors[0].message, "no_permission");
    }
    let schema = core.native_graphql().unwrap();
    let response = schema
        .execute(async_graphql::Request::new(
            "mutation{controlPeerStatus(action:STOP)}",
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["controlPeerStatus"],
        true
    );
    for action in ["START", "NOPE", "SNAPSHOT"] {
        let response = schema
            .execute(async_graphql::Request::new(format!(
                "mutation{{controlPeerStatus(action:{action})}}"
            )))
            .await;
        assert!(!response.errors.is_empty());
    }
    while let Ok(event) = events.try_recv() {
        assert_ne!(event.event_type, "CONTENT_CHANGED");
    }
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}
