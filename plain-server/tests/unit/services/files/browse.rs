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
    let forbidden = denied.to_string_lossy().into_owned();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let worker = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            assert_eq!(request["method"], "fileTaskAuthorize");
            let path = request["params"]["paths"][0].as_str().unwrap();
            facts.lock().unwrap().push(path.into());
            let response = if path == forbidden {
                json!({"id":request["id"],"error":"synthetic access denied"})
            } else {
                json!({"id":request["id"],"result":true})
            };
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
    variables["query"] = json!(format!("parent:{}", json!(denied)));
    let response = call(&native, query, variables.clone()).await;
    assert_eq!(response.errors.len(), 1);
    assert!(response.errors[0].message.contains("denied"));
    assert_eq!(
        paths.lock().unwrap().last().unwrap(),
        denied.to_str().unwrap()
    );
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
    assert_eq!(
        paths.lock().unwrap().last().unwrap(),
        archive.to_str().unwrap()
    );
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
            assert_eq!(request["method"], "fileTaskAuthorize");
            observed
                .lock()
                .unwrap()
                .push(request["params"]["paths"].clone());
            host.reply(generation, json!({"id":request["id"],"result":false}))
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
                .is_none()
        );
    }
    assert_eq!(*calls.lock().unwrap(), vec![json!([path]), json!([path])]);
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
    assert_eq!(calls.lock().unwrap().len(), 2);
    server.shutdown().await;
    worker.abort();
}
