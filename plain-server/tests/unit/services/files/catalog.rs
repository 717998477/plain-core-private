use super::*;

fn metadata() -> plain_rs::db::MediaFileMetadata {
    plain_rs::db::MediaFileMetadata {
        data_type: DataType::Image,
        path: "/catalog/picture.jpg".into(),
        title: "Picture".into(),
        artist: String::new(),
        bucket_id: "pictures".into(),
        size_bytes: 2048,
        duration_ms: 0,
        cover_image_id: None,
        width: 800,
        height: 600,
        rotation: 0,
        taken_at: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[tokio::test]
async fn native_catalog_reads_rows_counts_ids_and_buckets_without_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let stored = db.record_media_files(&[metadata()]).unwrap();
    let catalog = MediaCatalog::new(db, Arc::new(Host::default()));
    assert!(matches!(catalog, MediaCatalog::Native(_)));
    let rows = catalog
        .rows(DataType::Image, "", FileSortBy::NameAsc, 10, 0)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], stored[0].id);
    assert_eq!(rows[0]["width"], 800);
    assert_eq!(catalog.count(DataType::Image, "").await.unwrap(), 1);
    assert_eq!(catalog.count(DataType::Video, "").await.unwrap(), 0);
    assert_eq!(
        catalog.ids(DataType::Image, "", false).await.unwrap(),
        vec![stored[0].id.clone()]
    );
    assert!(
        catalog
            .ids(DataType::Image, "", true)
            .await
            .unwrap()
            .is_empty()
    );
    let buckets = catalog.buckets(DataType::Image).await.unwrap();
    assert_eq!(buckets[0]["id"], "pictures");
}

#[tokio::test]
async fn platform_catalog_reads_os_facts_without_a_local_catalogue() {
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let replies = host.clone();
    let worker = tokio::spawn(async move {
        let mut methods = Vec::new();
        for _ in 0..4 {
            let request = requests.recv().await.unwrap();
            assert_eq!(request["params"]["dataType"], "IMAGE");
            let method = request["method"].as_str().unwrap();
            let value = match method {
                "systemMediaRows" => {
                    assert_eq!(request["params"]["query"], "text:picture");
                    assert_eq!(request["params"]["offset"], 4);
                    assert_eq!(request["params"]["limit"], 8);
                    assert_eq!(request["params"]["sortBy"], "NAME_ASC");
                    json!([{"id":"42","path":"content://media/images/42"}])
                }
                "systemMediaCount" => json!(1),
                "systemMediaIdsFacts" => {
                    assert_eq!(request["params"]["fromTrash"], true);
                    json!(["42"])
                }
                "systemMediaBucketItemFacts" => {
                    json!([{"id":"camera","path":"content://media/images/42"}])
                }
                other => panic!("unexpected host operation {other}"),
            };
            methods.push(method.to_owned());
            replies
                .reply(generation, json!({"id":request["id"],"result":value}))
                .unwrap();
        }
        methods
    });
    let catalog = MediaCatalog::Platform(host);
    let rows = catalog
        .rows(DataType::Image, "text:picture", FileSortBy::NameAsc, 8, 4)
        .await
        .unwrap();
    assert_eq!(rows[0]["id"], "42");
    assert_eq!(catalog.count(DataType::Image, "").await.unwrap(), 1);
    assert_eq!(
        catalog.ids(DataType::Image, "", true).await.unwrap(),
        vec!["42"]
    );
    assert_eq!(
        catalog.buckets(DataType::Image).await.unwrap()[0]["id"],
        "camera"
    );
    assert_eq!(worker.await.unwrap().len(), 4);
}
