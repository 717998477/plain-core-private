use crate::{
    assembly::server::ContentServer,
    graphql::executor::{RequestContext, execute_graphql},
};
use serde_json::{Value, json};

async fn command(core: &ContentServer, input: Value) -> async_graphql::Response {
    core.native_graphql()
        .unwrap()
        .execute(
            async_graphql::Request::new(
                "mutation($input:DlnaReceiverInput!){controlDlnaReceiver(input:$input)}",
            )
            .variables(async_graphql::Variables::from_json(json!({"input":input}))),
        )
        .await
}
async fn set_enabled(core: &ContentServer, enabled: bool) -> async_graphql::Response {
    core.native_graphql()
        .unwrap()
        .execute(
            async_graphql::Request::new(
                "mutation($value:JSON!){setUserPref(key:\"dlna\",value:$value)}",
            )
            .variables(async_graphql::Variables::from_json(
                json!({"value":enabled}),
            )),
        )
        .await
}
async fn snapshot(core: &ContentServer) -> Value {
    let response = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new("{dlnaReceiver}"))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.into_json().unwrap()["dlnaReceiver"].clone()
}

#[tokio::test]
async fn player_observations_are_not_graphql_commands() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    for input in [
        json!({"action":"POSITION","positionMs":1,"durationMs":2}),
        json!({"action":"PLAYBACK","state":"PLAYING"}),
        json!({"action":"SEEK_CONSUMED"}),
        json!({"action":"STOPPED"}),
        json!({"action":"ACCEPT","remember":false,"positionMs":1}),
    ] {
        assert!(!command(&core, input).await.errors.is_empty());
    }
    core.stop_core().await;
}

#[tokio::test]
async fn receiver_uses_main_schema_state_and_refuses_stopped_operations() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    assert_eq!(snapshot(&core).await["enabled"], false);
    assert!(set_enabled(&core, true).await.errors.is_empty());
    assert_eq!(snapshot(&core).await["enabled"], true);
    assert_eq!(snapshot(&core).await["isRunning"], false);
    assert!(set_enabled(&core, false).await.errors.is_empty());
    assert_eq!(snapshot(&core).await["enabled"], false);
    let response = core
        .native_graphql()
        .unwrap()
        .execute(async_graphql::Request::new("{dlnaReceiver}"))
        .await;
    assert!(response.errors.is_empty());
    assert_eq!(
        response.data.into_json().unwrap()["dlnaReceiver"]["rules"],
        json!({"allowed":[],"denied":[]})
    );
    let executor = core.native_graphql().unwrap();
    core.stop_core().await;
    for query in [
        "{dlnaReceiver}",
        "mutation{controlDlnaReceiver(input:{action:RETRY})}",
    ] {
        assert!(
            !executor
                .execute(async_graphql::Request::new(query))
                .await
                .errors
                .is_empty()
        );
    }
}

#[tokio::test]
async fn typed_receiver_controls_reject_remote_and_malformed_input_without_host_calls() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let state = core.runtime_state();
    let (_generation, mut requests) = state.host.connect();
    for query in [
        "{dlnaReceiver}",
        "mutation{controlDlnaReceiver(input:{action:RETRY})}",
    ] {
        let response = execute_graphql(
            &state.public,
            async_graphql::Request::new(query),
            RequestContext::remote(Some(state.clone())),
        )
        .await;
        assert_eq!(response.errors.len(), 1);
        assert_eq!(response.errors[0].message, "no_permission");
    }
    for input in [
        json!({"action":"POSITION","positionMs":1}),
        json!({"action":"START","port":-1}),
        json!({"action":"START","port":65536}),
        json!({"action":"STOP","port":1234}),
        json!({"action":"SNAPSHOT"}),
        json!({"action":"SET_ENABLED","enabled":true}),
        json!({"action":"RETRY","port":7878}),
        json!({"action":"STOP","unexpected":true}),
    ] {
        assert!(
            !command(&core, input.clone()).await.errors.is_empty(),
            "{input}"
        );
    }
    assert!(requests.try_recv().is_err());
    core.stop_core().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn enabling_receiver_uses_the_public_listeners_actual_bound_port() {
    let directory = tempfile::tempdir().unwrap();
    let core = ContentServer::open(directory.path()).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    assert_ne!(http, 0);
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            if request["method"] == "systemCapabilities" {
                host.reply(
                    generation,
                    json!({"id":request["id"],"result":["LAN_SHARE"]}),
                )
                .unwrap();
                continue;
            }
            assert_eq!(request["method"], "mdnsMulticast");
            let acquire = request["params"]["acquire"].as_bool().unwrap();
            host.reply(generation, json!({"id":request["id"],"result":acquire}))
                .unwrap();
        }
    });
    let response = set_enabled(&core, true).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let value = snapshot(&core).await;
    assert_eq!(value["enabled"], true);
    assert_eq!(
        value["isRunning"], true,
        "{value}; command: {:?}",
        response.data
    );
    assert_eq!(value["port"], http);
    core.stop_public().await;
    assert_eq!(snapshot(&core).await["isRunning"], false);
    assert_eq!(snapshot(&core).await["enabled"], true);
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (restarted_http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let restarted = snapshot(&core).await;
    assert_eq!(restarted["isRunning"], true);
    assert_eq!(restarted["port"], restarted_http);
    assert!(set_enabled(&core, false).await.errors.is_empty());
    assert_eq!(snapshot(&core).await["isRunning"], false);
    core.shutdown().await;
}
