use super::*;
use plain_rs::db::Db;

async fn settled(runtime: &Runtime) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let loading = runtime
                .task
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|task| !task.is_finished());
            if !loading && !runtime.index.status().is_running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn failed_replacement_preserves_active_package_index_and_search() {
    let directory = tempfile::tempdir().unwrap();
    let incoming = directory.path().join("ai_models/incoming");
    let original = plain_inference::models::test_fixture::fixture(&incoming);
    let photo = directory.path().join("photo.png");
    image::RgbImage::from_pixel(64, 64, image::Rgb([128, 64, 32]))
        .save(&photo)
        .unwrap();
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let peer = host.clone();
    let task = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "systemImageModelsObserve"
                | "imageIndexVerify"
                | "imageIndexEnd"
                | "imageIndexProgress" => json!(true),
                "imageIndexBegin" => json!({"revision":"one","total":1}),
                "imageIndexPage" => {
                    json!({"revision":"one","items":[{"id":"one","path":photo}],"nextCursor":"one","done":true})
                }
                method => panic!("Unexpected {method}"),
            };
            peer.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    let prefs = Arc::new(Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&directory.path().join("data.db")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let index = Arc::new(ImageIndex::new(db.clone(), host.clone(), events.clone()));
    let runtime = Runtime::new(directory.path().into(), host, prefs, index, events);
    runtime.import().await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert_eq!(plain_rs::media::images::embeddings::count(&db).unwrap(), 1);
    assert_eq!(runtime.search("cat", 50).await.unwrap()[0].image_id, "one");
    let mut replacement = plain_inference::models::test_fixture::fixture(&incoming);
    replacement.files[0].sha256 = "0".repeat(64);
    std::fs::write(
        incoming.join("manifest.json"),
        serde_json::to_vec(&replacement).unwrap(),
    )
    .unwrap();
    runtime.import().await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert!(runtime.snapshot().status.error_message.contains("checksum"));
    assert_eq!(plain_rs::media::images::embeddings::count(&db).unwrap(), 1);
    let active = plain_inference::models::manifest::Manifest::parse(
        &std::fs::read(directory.path().join("ai_models/active/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        active.fingerprint().unwrap(),
        original.fingerprint().unwrap()
    );
    assert_eq!(runtime.search("cat", 50).await.unwrap()[0].score, 1.0);
    runtime.shutdown().await;
    runtime.enable(true).await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert!(runtime.snapshot().status.error_message.is_empty());
    assert_eq!(runtime.search("cat", 50).await.unwrap()[0].score, 1.0);
    replacement = plain_inference::models::test_fixture::fixture(&incoming);
    replacement.id = "different-space".into();
    std::fs::write(
        incoming.join("manifest.json"),
        serde_json::to_vec(&replacement).unwrap(),
    )
    .unwrap();
    runtime.import().await.unwrap();
    settled(&runtime).await;
    assert_eq!(
        runtime.snapshot().status.status,
        ImageSearchStatusType::Ready
    );
    assert_eq!(plain_rs::media::images::embeddings::count(&db).unwrap(), 1);
    runtime.cancel(true).await.unwrap();
    assert_eq!(plain_rs::media::images::embeddings::count(&db).unwrap(), 0);
    assert!(!directory.path().join("ai_models").exists());
    task.abort();
}

#[tokio::test]
async fn catalog_changes_share_one_cancellable_timer_and_do_not_retain_the_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let prefs = Arc::new(Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let index = Arc::new(ImageIndex::new(db, host.clone(), events.clone()));
    let runtime = Runtime::new(directory.path().into(), host.clone(), prefs, index, events);
    runtime.catalog_changed();
    assert!(runtime.catalog_task.lock().unwrap().is_none());
    runtime.observing.store(true, Ordering::Release);
    runtime.catalog_changed();
    let old = runtime
        .catalog_task
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .abort_handle();
    runtime.catalog_changed();
    let current = runtime
        .catalog_task
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .abort_handle();
    assert_ne!(old.id(), current.id());
    assert_eq!(Arc::strong_count(&runtime), 1);
    runtime.cancel_catalog_changes();
    assert!(runtime.catalog_task.lock().unwrap().is_none());
    tokio::time::timeout(Duration::from_secs(5), async {
        while !old.is_finished() || !current.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!runtime.index.status().is_running);
    assert!(requests.try_recv().is_err());
    host.disconnect(generation);
}
