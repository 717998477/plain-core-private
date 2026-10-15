use super::*;
use crate::assembly::server::ContentServer;
use async_graphql::{Request, Variables};
use plain_rs::chat::enums::{DeviceType, PeerStatus};
use plain_rs::db::{
    DChannel, DPeer,
    chat_store::{SaveMode, channels, peers},
};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn native_main_record_queries_preserve_addresses_keys_members_and_checked_file_urls() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "fixture").unwrap();
    let server = ContentServer::start(
        &directory.path().join("plain.db"),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let mut peer = DPeer::new("peer", "Device", "127.0.0.1", Some(8443), DeviceType::Phone);
    peer.key = plain_rs::base64_encode(&[7; 32]);
    peer.status = PeerStatus::Paired;
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let mut channel = DChannel::new("Channel", "fixture");
    channel.id = "channel".into();
    channel.key = "channel-key".into();
    channel.members = r#"[{"peerId":"peer","status":"JOINED"}]"#.into();
    channel.version = 5000000001;
    channels::save(&state.db, &[channel], SaveMode::Insert).unwrap();
    let native = server.native_graphql().unwrap();
    let response=native.execute(Request::new(r#"{ peerRecords(ids:["peer"]) { id key address { bestIp baseUrl apiUrl statusWsUrl } } peerRecord(id:"absent") { id } chatChannelRecord(id:"channel") { key ownerId members { peerId status } version } }"#)).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["peerRecords"][0]["key"], peer.key);
    assert_eq!(
        data["peerRecords"][0]["address"]["baseUrl"],
        "https://127.0.0.1:8443"
    );
    assert!(data["peerRecord"].is_null());
    assert_eq!(data["chatChannelRecord"]["version"], 5000000001_i64);
    assert_eq!(data["chatChannelRecord"]["key"], "channel-key");
    assert_eq!(data["chatChannelRecord"]["members"][0]["peerId"], "peer");
    let input = json!({"id":peer.id,"name":peer.name,"ip":peer.ip,"key":peer.key,"publicKey":peer.public_key,"status":"PAIRED","port":peer.port,"deviceType":"PHONE","token":peer.token,"createdAt":peer.created_at,"updatedAt":peer.updated_at});
    let request = || {
        Request::new("query($expected: PeerRecordInput!) { peerFileUrl(id: \"peer\", expected:$expected, fileId: \"a +?#%\") }").variables(Variables::from_json(json!({"expected":input.clone()})))
    };
    let response = native.execute(request()).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let url = reqwest::Url::parse(data["peerFileUrl"].as_str().unwrap()).unwrap();
    assert_eq!(
        url.query_pairs().find(|(key, _)| key == "id").unwrap().1,
        "a +?#%"
    );
    peer.key = plain_rs::base64_encode(&[8; 32]);
    peers::save(&state.db, &[peer], SaveMode::Update).unwrap();
    assert!(!native.execute(request()).await.errors.is_empty());
    let response=native.execute(Request::new(r#"mutation { removePeerRecord(id:"absent") deletePeer(id:"absent") unpairPeerRecord(id:"absent") unpairPeer(id:"absent") }"#)).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["removePeerRecord"], false);
    assert_eq!(data["deletePeer"], true);
    assert_eq!(data["unpairPeerRecord"], false);
    assert_eq!(data["unpairPeer"], true);
    state
        .db
        .with_conn(|conn| conn.execute_batch("DROP TABLE chat_channels;"))
        .unwrap();
    let response = native
        .execute(Request::new(
            r#"{ chatChannelRecord(id:"channel") { id } }"#,
        ))
        .await;
    assert!(!response.errors.is_empty());
    server.shutdown().await;
}
