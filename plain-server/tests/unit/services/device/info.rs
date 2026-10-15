use super::*;
use serde_json::json;
use std::sync::Arc;

#[test]
fn cpu_model_parser_preserves_the_pre_migration_fallback() {
    assert_eq!(
        cpu_model("processor: 0\nmodel name: Example CPU\nprocessor: 1\nmodel name: Other CPU\n"),
        Some("Example CPU".into())
    );
    assert_eq!(cpu_model("CPU part: 0x805\n"), None);
    assert_eq!(cpu_model("model name: "), None);
}

#[tokio::test]
async fn selected_properties_use_one_exchange_and_refresh_mutable_values() {
    let host = Arc::new(Host::default());
    let (generation, mut requests) = host.connect();
    let worker = {
        let host = host.clone();
        tokio::spawn(async move {
            for (language, density) in [("en", 2.0), ("fr", 3.0)] {
                let request = requests.recv().await.unwrap();
                assert_eq!(request["method"], "systemProperties");
                assert_eq!(
                    request["params"],
                    json!({"names":["language","display.density"]})
                );
                host.reply(generation, json!({"id":request["id"],"result":{"language":language,"display.density":density}})).unwrap();
            }
            assert!(requests.try_recv().is_err());
        })
    };
    for (language, density) in [("en", 2.0), ("fr", 3.0)] {
        let values = read_properties(&host, vec!["language".into(), "display.density".into()])
            .await
            .unwrap();
        let info = apply_sdk(
            direct(&[], std::path::Path::new("/")).unwrap(),
            values,
            true,
            false,
        )
        .unwrap();
        assert_eq!(info.language, language);
        assert_eq!(info.display.unwrap().density, density);
    }
    worker.await.unwrap();
}

#[tokio::test]
async fn invalid_or_missing_primitive_values_are_errors() {
    for result in [json!({}), json!({"language":42}), json!([])] {
        let host = Arc::new(Host::default());
        let (generation, mut requests) = host.connect();
        let worker = {
            let host = host.clone();
            tokio::spawn(async move {
                let request = requests.recv().await.unwrap();
                host.reply(generation, json!({"id":request["id"],"result":result}))
                    .unwrap();
            })
        };
        assert!(
            read_properties(&host, vec!["language".into()])
                .await
                .is_err()
        );
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn direct_only_selection_never_requests_platform_properties() {
    let host = Host::default();
    assert!(read_properties(&host, Vec::new()).await.unwrap().is_empty());
    assert!(
        sdk_properties(&[
            "kernelVersion".into(),
            "totalStorage".into(),
            "totalMemory".into()
        ])
        .is_empty()
    );
    let info = collect(
        &host,
        std::path::PathBuf::from("/"),
        vec!["platform".into()],
    )
    .await
    .unwrap();
    assert_eq!(info.platform, platform());
}

#[test]
fn nested_type_selection_preserves_nullable_object_presence() {
    let info = apply_sdk(
        direct(&[], std::path::Path::new("/")).unwrap(),
        Map::new(),
        true,
        true,
    )
    .unwrap();
    assert!(info.display.is_some());
    assert!(info.android.is_some());
    let info = apply_sdk(
        direct(&[], std::path::Path::new("/")).unwrap(),
        Map::new(),
        false,
        false,
    )
    .unwrap();
    assert!(info.display.is_none());
    assert!(info.android.is_none());
}

#[test]
fn raw_build_milliseconds_and_soc_fallback_map_in_rust() {
    let mut direct_info = direct(&[], std::path::Path::new("/")).unwrap();
    direct_info.cpu_model = Some("Proc CPU".into());
    let values = json!({"socModel":"unknown", "android.sdkVersion":35, "android.buildTime":1700000000123i64}).as_object().unwrap().clone();
    let info = apply_sdk(direct_info, values, false, true).unwrap();
    assert_eq!(info.cpu_model.as_deref(), Some("Proc CPU"));
    let android = info.android.unwrap();
    assert_eq!(android.sdk_version, 35);
    assert_eq!(android.build_time.0.timestamp_millis(), 1700000000123);
}

#[tokio::test]
async fn main_device_fields_keep_alias_fragment_and_nested_type_behavior_without_host_calls() {
    let host = Arc::new(Host::default());
    let (_generation, mut requests) = host.connect();
    let directory = tempfile::tempdir().unwrap();
    let db = Arc::new(plain_rs::db::Db::open(&directory.path().join("plain.db")).unwrap());
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema = crate::graphql::schemas::public::build(
        host,
        events,
        prefs,
        db,
        directory.path().to_path_buf(),
    );
    let response = schema.execute("query { first: deviceInfo { ...Identity } second: deviceInfo { platform } } fragment Identity on DeviceInfo { platform display { __typename } android { __typename } }").await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["first"]["platform"], data["second"]["platform"]);
    if cfg!(any(target_os = "android", target_os = "ios")) {
        assert_eq!(data["first"]["display"]["__typename"], "DisplayInfo");
    } else {
        assert!(data["first"]["display"].is_null());
    }
    if cfg!(target_os = "android") {
        assert_eq!(data["first"]["android"]["__typename"], "AndroidExtras");
    } else {
        assert!(data["first"]["android"].is_null());
    }
    assert!(requests.try_recv().is_err());
}
