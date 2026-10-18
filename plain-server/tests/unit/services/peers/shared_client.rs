use super::*;
use crate::assembly::server::ContentServer;
use crate::services::peers::shared_download::download_url;
use plain_rs::db::DChat;
use plain_rs::db::DPeer;
use plain_rs::db::chat_store::SaveMode;
use plain_rs::db::chat_store::messages;
use plain_rs::db::chat_store::peers;
use plain_rs::files::shares::client::Link;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn root() -> (tempfile::TempDir, ContentServer, String) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "local").unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[4; 32]);
    let server = ContentServer::start(dir.path(), &token, prefs).unwrap();
    (dir, server, token)
}
fn card(port: u16) -> Card {
    serde_json::from_value(json!({"shareId":"fixture","urlToken":plain_rs::utils::base64::base64_encode_url_safe(&[7;32]),"peerInfo":{"id":"peer","ip":"127.0.0.1","port":port},"name":"old","itemCount":2,"totalSize":8,"expiresAt":"2030-01-01T00:00:00.000Z"})).unwrap()
}
fn save_card(state: &ServerState, card: &Card) -> DChat {
    let row = DChat::new(
        "peer",
        "me",
        "",
        &json!({"type":"SHARE","value":card}).to_string(),
    );
    messages::save(&state.db, &[row.clone()], SaveMode::Insert).unwrap();
    row
}
async fn tls() -> (u16, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    use axum::{
        Router,
        body::Bytes,
        extract::Query,
        http::HeaderMap,
        routing::{get, post},
    };
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let config = axum_server::tls_rustls::RustlsConfig::from_pem(
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let router=Router::new().route("/guest_graphql",post(move |headers:HeaderMap,body:Bytes|{let count=seen.clone();async move {
        assert_eq!(headers["c-id"],"fixture");let plain=plain_rs::crypto::xchacha_decrypt_raw(&[7;32],&body).unwrap();let plain=String::from_utf8(plain).unwrap();let parts:Vec<_>=plain.splitn(3,'|').collect();assert!(uuid::Uuid::parse_str(parts[1]).is_ok());let request:Value=serde_json::from_str(parts[2]).unwrap();count.fetch_add(1,Ordering::SeqCst);
        if request["variables"]["virtualPath"]=="slow" {tokio::time::sleep(Duration::from_millis(200)).await;}
        let info=json!({"data":{"sharedInfo":{"name":"current","readOnly":true,"requiresPassword":false,"expiresAt":null,"urlToken":plain_rs::utils::base64::base64_encode(&[8;32]),"entries":[{"name":"a 中文.txt","virtualPath":"folder/ +?#中文%","isDir":false,"size":8,"mimeType":"text/plain","hasThumb":false}]}}});
        plain_rs::crypto::xchacha_encrypt_raw(&[7;32],info.to_string().as_bytes()).unwrap()
    }})).route("/fs",get(|Query(params):Query<std::collections::HashMap<String,String>>|async move {
        assert_eq!(params["sid"],"fixture");let plain=plain_rs::crypto::xchacha_decrypt_raw(&[8;32],&plain_rs::utils::base64::base64_decode(&params["id"])).unwrap();let decoded:Value=serde_json::from_slice(&plain).unwrap();assert_eq!(decoded["virtualPath"],"folder/ +?#中文%");"download"
    }));
    let task = tokio::spawn(async move {
        axum_server::from_tcp_rustls(socket, config)
            .serve(router.into_make_service())
            .await
            .unwrap();
    });
    (port, task, count)
}
#[tokio::test]
async fn authenticated_main_browse_caches_and_queues_real_tls_files_without_host() {
    use crate::test_support::graphql_http_variables;
    let (directory, server, token) = root();
    let state = server.runtime_state();
    let (port, tls, _) = tls().await;
    let expected = card(port);
    let row = save_card(&state, &expected);
    let client = reqwest::Client::new();
    let query = "mutation($id:ID!,$expected:SharedMessageCardInput!){browseSharedLink(messageId:$id,expected:$expected){info{urlToken} link{host port sharedId token pageUrl} card{name expiresAt totalSize}}}";
    let variables = json!({"id":row.id,"expected":expected});
    assert_eq!(
        client
            .post(format!("http://127.0.0.1:{}/graphql", server.port))
            .header("content-type", "application/json")
            .body(json!({"query":query,"variables":variables}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = graphql_http_variables(server.port, &token, query, variables.clone()).await;
    assert!(response.get("errors").is_none(), "{response}");
    let result = &response["data"]["browseSharedLink"];
    assert_eq!(result["card"]["name"], "current");
    assert!(result["card"]["expiresAt"].is_null());
    assert_eq!(result["card"]["totalSize"], 8);
    let link: Link = serde_json::from_value(result["link"].clone()).unwrap();
    let cached = graphql_http_variables(server.port, &token,
        "mutation($link:SharedLinkInput!,$urlToken:String!,$entry:SharedEntryInput!){cacheSharedFile(link:$link,urlToken:$urlToken,entry:$entry,thumbnail:false)}",
        json!({"link":link,"urlToken":result["info"]["urlToken"],"entry":{"name":"preview.txt","virtualPath":"folder/ +?#中文%","isDir":false,"size":8,"mimeType":"text/plain","hasThumb":false}})).await;
    assert!(cached.get("errors").is_none(), "{cached}");
    assert_eq!(
        tokio::fs::read(cached["data"]["cacheSharedFile"].as_str().unwrap())
            .await
            .unwrap(),
        b"download"
    );
    assert_eq!(state.lan.capacity.available_permits(), 4);
    let stale = graphql_http_variables(server.port, &token, query, variables).await;
    assert!(stale.get("errors").is_some(), "{stale}");
    let link = Link::new(
        "127.0.0.1",
        port,
        "fixture",
        &plain_rs::utils::base64::base64_encode_url_safe(&[7; 32]),
    )
    .unwrap();
    let target = directory.path().join("queued");
    let queued = graphql_http_variables(
        server.port,
        &token,
        "mutation($input: SharedDownloadInput!) { enqueueSharedDownload(input: $input) }",
        json!({"input":{
            "messageId":row.id,"kind":"SYNC","link":link,
            "urlToken":result["info"]["urlToken"],
            "entries":[{"name":"Root","virtualPath":"root","isDir":true,"size":0,"mimeType":"","hasThumb":false}],
            "targetDir":target.to_str().unwrap(),"zipName":""
        }}),
    ).await;
    assert!(queued.get("errors").is_none(), "{queued}");
    let id = queued["data"]["enqueueSharedDownload"].as_str().unwrap();
    let mut changed = state.shared_batches.queue.changed.subscribe();
    let completed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = state.shared_batches.queue.snapshot();
            let task = snapshot["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|task| task["id"] == id)
                .unwrap();
            if matches!(
                task["status"].as_str(),
                Some("COMPLETED" | "PARTIAL" | "FAILED" | "CANCELED")
            ) {
                return task.clone();
            }
            changed.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(completed["status"], "COMPLETED", "{completed}");
    assert_eq!(completed["totalSize"], 8);
    assert_eq!(completed["totalFiles"], 1);
    assert_eq!(completed["doneFiles"], 1);
    assert_eq!(
        tokio::fs::read(target.join("Root/a 中文.txt"))
            .await
            .unwrap(),
        b"download"
    );
    assert!(!state.host.connected());
    server.shutdown().await;
    tls.abort();
}
#[tokio::test]
async fn discovery_retry_rereads_peer_address_and_late_response_cannot_replace_card() {
    let (_dir, server, _) = root();
    let state = server.runtime_state();
    let (port, tls, count) = tls().await;
    let expected = card(1);
    let row = save_card(&state, &expected);
    let peer = DPeer::new(
        "peer",
        "peer",
        "",
        Some(1),
        plain_rs::chat::enums::DeviceType::Phone,
    );
    peers::save(&state.db, &[peer.clone()], SaveMode::Insert).unwrap();
    let task = {
        let state = state.clone();
        let expected = expected.clone();
        let id = row.id.clone();
        tokio::spawn(async move { browse(&state, &id, &expected, None).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut peer = peer;
    peer.ip = "invalid,127.0.0.1".into();
    peer.port = Some(port);
    peers::save(&state.db, &[peer], SaveMode::Update).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["link"]["port"], port);
    let current: Card = serde_json::from_value(result["card"].clone()).unwrap();
    let before = count.load(Ordering::SeqCst);
    let task = {
        let state = state.clone();
        let id = row.id.clone();
        tokio::spawn(async move { browse(&state, &id, &current, Some("slow")).await })
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        while count.load(Ordering::SeqCst) == before {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    messages::content(
        &state.db,
        &row.id,
        "{\"type\":\"TEXT\",\"value\":{\"text\":\"replacement\"}}",
    )
    .unwrap();
    assert!(task.await.unwrap().is_err());
    assert!(
        messages::get(&state.db, &row.id)
            .unwrap()
            .unwrap()
            .content
            .contains("replacement")
    );
    server.shutdown().await;
    tls.abort();
}
#[test]
fn discovery_selects_complete_matching_records_and_safe_ipv6_urls() {
    let expected = card(443);
    let mut links = vec![];
    let mut seen = HashSet::new();
    let snapshot = json!({"services":[{"complete":false,"port":8443,"txtRecords":["id=peer"],"ips":["127.0.0.1"]},{"complete":true,"port":8443,"txtRecords":["id=other"],"ips":["127.0.0.1"]},{"complete":true,"port":8443,"txtRecords":["id=peer"],"ips":["127.0.0.1","127.0.0.1"],"ipv6":["::1"]}]});
    discovery_candidates(&mut links, &mut seen, &snapshot, &expected);
    assert_eq!(links.len(), 2);
    assert!(links[1].page_url.starts_with("https://[::1]:8443/"));
    assert!(download_url("http://localhost/fs?sid=s&id=x").is_err());
    assert!(download_url("https://user@localhost/fs?sid=s&id=x").is_err());
    assert!(download_url("https://localhost/proxyfs?sid=s&id=x").is_err());
}

#[tokio::test]
async fn main_share_links_use_root_secret_ports_and_revocation_without_host() {
    let (dir, server, token) = root();
    let state = server.runtime_state();
    state
        .prefs
        .set(
            "master_secret",
            plain_rs::utils::base64::base64_encode(&[5; 32]),
        )
        .unwrap();
    state.prefs.set_user("https_port", 2443).unwrap();
    let path = dir.path().join("file.txt");
    std::fs::write(&path, b"fixture").unwrap();
    let service = plain_rs::files::shares::Service::new(state.db.clone(), state.prefs.clone());
    let share = service
        .create(
            "fixture".into(),
            vec![path.to_string_lossy().into_owned()],
            true,
            None,
        )
        .unwrap();
    let query = r#"query($id:ID!){shareLink(id:$id,host:"::1"){token pageUrl}}"#;
    let variables = json!({"id":share.id});
    let response =
        crate::test_support::graphql_http_variables(server.port, &token, query, variables.clone())
            .await;
    assert!(response.get("errors").is_none(), "{response}");
    assert_eq!(
        response["data"]["shareLink"]["token"],
        service.token(&share.id).unwrap()
    );
    assert!(
        response["data"]["shareLink"]["pageUrl"]
            .as_str()
            .unwrap()
            .starts_with("https://[::1]:2443/s/")
    );
    state.db.share_delete(&share.id).unwrap();
    let response =
        crate::test_support::graphql_http_variables(server.port, &token, query, variables).await;
    assert!(response.get("errors").is_some(), "{response}");
    assert!(!state.host.connected());
    server.shutdown().await;
}
#[tokio::test]
async fn root_candidates_preserve_foreign_self_unbound_and_deduplicated_endpoint_cases() {
    let (_dir, server, _) = root();
    let state = server.runtime_state();
    let mut expected = card(8443);
    expected.peer_info.ip = "10.0.0.9".into();
    let foreign = candidates(&state, &expected, false).unwrap();
    assert_eq!(foreign.len(), 1);
    assert_eq!(foreign[0].host, "10.0.0.9");
    assert_eq!(foreign[0].port, 8443);
    expected.peer_info.id = "local".into();
    state.prefs.set_user("https_port", 9999).unwrap();
    let own = candidates(&state, &expected, false).unwrap();
    assert_eq!(own[0].host, "127.0.0.1");
    assert_eq!(own[0].port, 9999);
    assert!(
        own.iter()
            .any(|link| link.host == "10.0.0.9" && link.port == 8443)
    );
    state.prefs.set_user("https_port", 0).unwrap();
    let unbound = candidates(&state, &expected, false).unwrap();
    assert_eq!(unbound.len(), 1);
    assert_eq!(unbound[0].host, "10.0.0.9");
    assert_eq!(unbound[0].port, 8443);
    state.prefs.set_user("https_port", 9999).unwrap();
    expected.peer_info.ip = "127.0.0.1".into();
    expected.peer_info.port = 9999;
    assert_eq!(
        candidates(&state, &expected, false)
            .unwrap()
            .iter()
            .filter(|link| link.host == "127.0.0.1" && link.port == 9999)
            .count(),
        1
    );
    server.shutdown().await;
}
