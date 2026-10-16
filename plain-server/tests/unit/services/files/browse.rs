use crate::assembly::server::ContentServer;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn main_browse_authorizes_effective_parent_and_owns_pagination_count_and_zip_filtering() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("allowed");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.txt"), b"1234").unwrap();
    std::fs::write(root.join("b.txt"), b"12").unwrap();
    let denied = temp.path().join("denied");
    std::fs::create_dir(&denied).unwrap();
    std::fs::write(denied.join("private.txt"), b"private").unwrap();
    let server = ContentServer::open(&temp.path().join("core")).unwrap();
    let state = server.runtime_state();
    state.file_platform.use_sdk_for_test();
    let paths = Arc::new(Mutex::new(Vec::<String>::new()));
    let facts = paths.clone();
    let revoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let deny = revoked.clone();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let worker = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            assert_eq!(request["method"], "systemPermissionFacts");
            assert_eq!(
                request["params"]["permissions"],
                json!(["WRITE_EXTERNAL_STORAGE"])
            );
            facts.lock().unwrap().push("WRITE_EXTERNAL_STORAGE".into());
            let response = json!({"id":request["id"],"result":{"granted":{"WRITE_EXTERNAL_STORAGE":!deny.load(std::sync::atomic::Ordering::SeqCst)}}});
            host.reply(generation, response).unwrap();
        }
    });
    let native = server.native_graphql().unwrap();
    let query = "query($root:String!,$query:String!,$offset:Int!,$countOnly:Boolean!){browseFiles(root:$root,query:$query,offset:$offset,limit:1,countOnly:$countOnly,sortBy:NAME_ASC){count items{name size path}}}";
    async fn call(
        native: &crate::native::graphql::NativeGraphqlExecutor,
        query: &str,
        variables: Value,
    ) -> async_graphql::Response {
        native
            .execute(
                async_graphql::Request::new(query)
                    .variables(async_graphql::Variables::from_json(variables)),
            )
            .await
    }
    let mut variables = json!({"root":root,"query":"","offset":1,"countOnly":false});
    let response = call(&native, query, variables.clone()).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let page = response.data.into_json().unwrap();
    assert_eq!(page["browseFiles"]["count"], 2);
    assert_eq!(page["browseFiles"]["items"][0]["name"], "b.txt");
    variables["countOnly"] = json!(true);
    let response = call(&native, query, variables.clone()).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let page = response.data.into_json().unwrap();
    assert_eq!(page["browseFiles"]["count"], 2);
    assert!(page["browseFiles"]["items"].as_array().unwrap().is_empty());
    revoked.store(true, std::sync::atomic::Ordering::SeqCst);
    variables["query"] = json!(format!("parent:{}", json!(denied)));
    let response = call(&native, query, variables.clone()).await;
    assert_eq!(response.errors.len(), 1);
    assert!(response.errors[0].message.contains("denied"));
    revoked.store(false, std::sync::atomic::Ordering::SeqCst);
    let archive = temp.path().join("archive.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    use std::io::Write;
    for (name, content) in [
        ("small", b"12".as_slice()),
        ("large", b"12345678".as_slice()),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap();
    variables = json!({"root":format!("{}!zip!/",archive.display()),"query":"file_size:>4","offset":0,"countOnly":false});
    let response = call(&native, query, variables).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let page = response.data.into_json().unwrap();
    assert_eq!(page["browseFiles"]["count"], 1);
    assert_eq!(page["browseFiles"]["items"][0]["name"], "large");
    assert!(paths.lock().unwrap().len() >= 4);
    server.shutdown().await;
    worker.abort();
}

#[tokio::test]
async fn stat_authorizes_the_resolved_path_once_and_rejects_invalid_references_before_host_access()
{
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("core");
    let server = ContentServer::open(&directory).unwrap();
    let state = server.runtime_state();
    state.file_platform.use_sdk_for_test();
    let path = directory.join("example.txt");
    std::fs::write(&path, b"example").unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = calls.clone();
    let host = state.host.clone();
    let (generation, mut incoming) = host.connect();
    let worker = tokio::spawn(async move {
        while let Some(request) = incoming.recv().await {
            assert_eq!(request["method"], "systemPermissionFacts");
            observed
                .lock()
                .unwrap()
                .push(request["params"]["permissions"].clone());
            host.reply(
                generation,
                json!({"id":request["id"],"result":{"granted":{"WRITE_EXTERNAL_STORAGE":false}}}),
            )
            .unwrap();
        }
    });
    for reference in [
        "app://example.txt".to_owned(),
        format!("file://{}", path.display()),
    ] {
        assert!(
            crate::services::files::browse::stat_record(&state.file_platform, &reference)
                .await
                .is_some()
        );
    }
    assert!(calls.lock().unwrap().is_empty());
    let external = temp.path().join("external.txt");
    std::fs::write(&external, b"external").unwrap();
    assert!(
        crate::services::files::browse::stat_record(
            &state.file_platform,
            external.to_str().unwrap()
        )
        .await
        .is_none()
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec![json!(["WRITE_EXTERNAL_STORAGE"])]
    );
    for reference in [
        "",
        " ",
        ".",
        "app://../example.txt",
        "fid:invalid",
        "file://remote/example.txt",
    ] {
        assert!(
            crate::services::files::browse::stat_record(&state.file_platform, reference)
                .await
                .is_none()
        );
    }
    assert_eq!(calls.lock().unwrap().len(), 1);
    server.shutdown().await;
    worker.abort();
}
