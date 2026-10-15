use super::*;
use crate::assembly::server::ContentServer;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn native_main_schema_reads_and_stops_idle_mdns_without_initializing_the_host() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "mdns-fixture").unwrap();
    let server = ContentServer::start(
        &directory.path().join("plain.db"),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let native = server.native_graphql().unwrap();
    let response = native.execute(async_graphql::Request::new("{ mdnsState { runtimeId revision receiver running scanning capturing debugSessionCount services { instanceFqdn } packetsIn { time } packetsOut { time } } }")).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let state = &data["mdnsState"];
    assert!(!state["runtimeId"].as_str().unwrap().is_empty());
    assert_eq!(state["revision"], 0);
    assert_eq!(state["receiver"], false);
    assert_eq!(state["services"], json!([]));
    assert_eq!(state["packetsIn"], json!([]));
    let response = native
        .execute(async_graphql::Request::new(
            "mutation { stopMdnsDiscovery { runtimeId revision receiver } }",
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let stopped = response.data.into_json().unwrap();
    assert_eq!(
        stopped["stopMdnsDiscovery"]["runtimeId"],
        state["runtimeId"]
    );
    assert_eq!(stopped["stopMdnsDiscovery"]["receiver"], false);
    let response = native.execute(async_graphql::Request::new("mutation { prewarmPeerTransport(id: \"unknown-peer\") { shortId awareSupported awareRunning } }")).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert!(response.data.into_json().unwrap()["prewarmPeerTransport"].is_null());
    server.shutdown().await;
}

#[test]
fn mdns_snapshot_projection_keeps_packet_timestamps_and_ipv6_without_loss() {
    let state: MdnsState = serde_json::from_value(json!({
        "runtimeId":"fixture", "revision":5000000000_i64, "receiver":true,"running":true,"scanning":false,"published":true,"manualScanning":false,"debugSessionCount":1,"capturing":true,
        "services":[{"serviceType":"_plain._tcp.local.","instanceName":"fixture","instanceFqdn":"fixture._plain._tcp.local.","hostname":"fixture.local.","port":8443,"txtRecords":["id=fixture"],"ips":[],"ipv6":["fe80::1%4"],"complete":true}],
        "packetsIn":[{"time":1790812800123_i64,"direction":"IN","srcIp":"::1","srcPort":5353,"dstIp":"ff02::fb","dstPort":5353,"size":100,"summary":"PTR","detail":"fixture"}],"packetsOut":[]
    })).unwrap();
    assert_eq!(state.revision, Long(5000000000));
    assert_eq!(state.packets_in[0].time, Long(1790812800123));
    assert_eq!(state.services[0].ipv6, ["fe80::1%4"]);
}
