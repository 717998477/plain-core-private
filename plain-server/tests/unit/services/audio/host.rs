use super::*;
use serde_json::json;

#[tokio::test]
async fn progress_updates_do_not_resolve_the_catalog_or_increment_play_counts() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let lookups = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = lookups.clone();
    let provider = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "audioLibraryResolve" => {
                    counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    json!([{"mediaId":"42","path":"/one","title":"One","artist":"Test","albumId":"7","durationMs":117123}])
                }
                "audioEngineCommand" => {
                    let mut result = request["params"]["playback"].clone();
                    result["loaded"] = request["params"]["newTrack"].clone();
                    result
                }
                method => panic!("unexpected {method}"),
            };
            host.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    let executor = core.native_graphql().unwrap();
    let response = executor
        .execute_private(r#"mutation { audioPlayTrack(mediaId:"42") { revision } }"#.into())
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let revision = response.data.into_json().unwrap()["audioPlayTrack"]["revision"]
        .as_i64()
        .unwrap();
    let response=executor.execute_private(format!(r#"mutation {{ audioReportStarted(track:{{path:"/one",title:"One",artist:"Test",albumId:"7",durationMs:117123}},revision:{revision}) }}"#).into()).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["audioReportStarted"],
        true
    );
    let before = lookups.load(std::sync::atomic::Ordering::SeqCst);
    let writes = state.db.with_conn(|conn| conn.total_changes());
    for position in [123_i64, 3_000_000_123] {
        let response=executor.execute_private(format!(r#"mutation {{ audioReportProgress(path:"/one",revision:{revision},positionMs:{position}) }}"#).into()).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(
            response.data.into_json().unwrap()["audioReportProgress"],
            true
        );
    }
    assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), before);
    assert_eq!(state.db.with_conn(|conn| conn.total_changes()), writes + 2);
    let row = state.db.media_play_history_get("42").unwrap().unwrap();
    assert_eq!(row.play_count, 1);
    assert_eq!(row.position_ms, 3_000_000_123);
    let response = executor
        .execute_private(
            format!(
                r#"mutation {{ audioReportProgress(path:"/one",revision:{},positionMs:0) }}"#,
                revision - 1
            )
            .into(),
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["audioReportProgress"],
        false
    );
    assert_eq!(state.db.with_conn(|conn| conn.total_changes()), writes + 2);
    assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), before);
    provider.abort();
    core.stop_core().await;
}
