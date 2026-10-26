use crate::assembly::server::ContentServer;
use crate::host::headless::runtime::Runtime;
use crate::native::events::NativeEventStream;
use crate::native::graphql::{NativeGraphqlExecutor, NativeRequestScope};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

struct Node {
    core: Arc<ContentServer>,
    host: tokio::task::JoinHandle<Result<(), String>>,
    native: NativeGraphqlExecutor,
    events: NativeEventStream,
    scope: NativeRequestScope,
    port: u16,
    _directory: tempfile::TempDir,
}

impl Node {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        std::fs::create_dir(&data).unwrap();
        std::fs::write(
            data.join("prefs.json"),
            json!({
                "device_name":"Acceptance fixture", "desktop_access":true,
            })
            .to_string(),
        )
        .unwrap();
        let core = Arc::new(
            ContentServer::open_with_cache(&data, &directory.path().join("cache")).unwrap(),
        );
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let (_, port) = core
            .start_public(
                0,
                0,
                certificate.cert.pem().into_bytes(),
                certificate.key_pair.serialize_pem().into_bytes(),
            )
            .await
            .unwrap();
        let host = tokio::spawn(Arc::new(Runtime::new(core.clone(), vec![])).run());
        let native = core.native_graphql().unwrap();
        let scope = NativeRequestScope::default();
        let events = native.subscribe_events(scope.clone()).unwrap();
        Self {
            core,
            host,
            native,
            events,
            scope,
            port,
            _directory: directory,
        }
    }

    async fn close(mut self) {
        self.core.close_native_scope(&self.scope).await.unwrap();
        self.core.stop_core().await;
        (&mut self.host).await.unwrap().unwrap();
    }

    async fn event(&mut self, expected: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(15), async {
            for _ in 0..128 {
                let event = self.events.recv().await.unwrap();
                if event.event_type == expected {
                    return serde_json::from_str(&event.payload).unwrap();
                }
            }
            panic!("expected native event {expected}");
        })
        .await
        .unwrap()
    }

    async fn paired(&mut self, generation: &str) {
        tokio::time::timeout(Duration::from_secs(15), async {
            for _ in 0..128 {
                let data = query(
                    &self.native,
                    "{pairingStates{generation phase error}}",
                    json!({}),
                )
                .await;
                let state = data["pairingStates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|state| state["generation"] == generation)
                    .unwrap();
                if state["phase"] == "SUCCEEDED" {
                    return;
                }
                assert_ne!(state["phase"], "FAILED", "{state}");
                assert_ne!(state["phase"], "CANCELLED", "{state}");
                self.events.recv().await.unwrap();
            }
            panic!("pairing did not converge");
        })
        .await
        .unwrap();
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.scope.close();
        self.host.abort();
    }
}

async fn query(native: &NativeGraphqlExecutor, document: &str, variables: Value) -> Value {
    let response = native
        .execute(
            async_graphql::Request::new(document)
                .variables(async_graphql::Variables::from_json(variables)),
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.into_json().unwrap()
}

#[tokio::test]
async fn native_password_and_approval_pairing_enables_actual_chat_and_preserves_the_third_node() {
    let mut first = Node::start().await;
    let mut second = Node::start().await;
    let third = Node::start().await;
    query(
        &second.native,
        r#"mutation{
        passwordType:setPref(key:"password_type",value:"FIXED")
        password:setPref(key:"password",value:"fixture-password")
        twoFactor:setPref(key:"login_require_approval",value:true)
    }"#,
        json!({}),
    )
    .await;
    let started = query(
        &first.native,
        r#"mutation($host:String!){pairDevice(input:{
        host:$host,mode:MANAGE,password:"fixture-password"
    }){generation}}"#,
        json!({"host":format!("127.0.0.1:{}", second.port)}),
    )
    .await;
    let requested = second.event("WEB_LOGIN_REQUESTED").await;
    second
        .core
        .native_management()
        .unwrap()
        .confirm_web_login(requested["requestId"].as_str().unwrap().into(), true)
        .await
        .unwrap();
    first
        .paired(started["pairDevice"]["generation"].as_str().unwrap())
        .await;
    let peers = "{peers{id status key token publicKey}}";
    let first_peers = query(&first.native, peers, json!({})).await;
    let second_peers = query(&second.native, peers, json!({})).await;
    assert_eq!(first_peers["peers"].as_array().unwrap().len(), 1);
    assert_eq!(second_peers["peers"].as_array().unwrap().len(), 1);
    let first_id = first.native.client_id();
    let second_id = second.native.client_id();
    assert_eq!(first_peers["peers"][0]["id"], second_id);
    assert_eq!(second_peers["peers"][0]["id"], first_id);
    assert_eq!(first_peers["peers"][0]["status"], "PAIRED");
    assert_eq!(second_peers["peers"][0]["status"], "PAIRED");
    assert_eq!(
        first_peers["peers"][0]["key"],
        second_peers["peers"][0]["key"]
    );
    assert_eq!(
        plain_rs::utils::base64::base64_decode(first_peers["peers"][0]["token"].as_str().unwrap())
            .len(),
        32
    );
    let content =
        json!({"type":"TEXT","value":{"text":"accepted native pairing message"}}).to_string();
    let sent = query(&first.native, "mutation($target:String!,$content:String!){sendChatItem(target:$target,content:$content){id status}}",
        json!({"target":format!("peer:{second_id}"),"content":content})).await;
    assert_eq!(sent["sendChatItem"][0]["status"], "SENT");
    let received = query(&second.native, "query($target:String!){chatItems(target:$target,query:\"\",offset:0,limit:10){fromId toId content}}",
        json!({"target":format!("peer:{first_id}")})).await;
    assert_eq!(
        received["chatItems"],
        json!([{"fromId":first_id,"toId":"me","content":content}])
    );
    assert_eq!(
        query(
            &third.native,
            "{peers{id} chatConversations{id}}",
            json!({})
        )
        .await,
        json!({"peers":[],"chatConversations":[]})
    );
    first.close().await;
    second.close().await;
    third.close().await;
}
