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
        startup(&host).unwrap(),
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

#[tokio::test]
async fn fixed_device_information_is_reused_and_storage_remains_live() {
    let directory = tempfile::tempdir().unwrap();
    let host = Host::default();
    let mut fixed = direct(&[], directory.path()).unwrap();
    fixed.app_version = "startup-version".into();
    fixed.total_memory = Long(123456);
    fixed.total_storage = Long(-1);
    let info = collect(
        &host,
        directory.path().to_path_buf(),
        vec![
            "appVersion".into(),
            "totalMemory".into(),
            "totalStorage".into(),
        ],
        fixed,
    )
    .await
    .unwrap();
    assert_eq!(info.app_version, "startup-version");
    assert_eq!(info.total_memory.0, 123456);
    assert!(info.total_storage.0 >= 0);
}

#[tokio::test]
async fn fixed_device_information_is_reused_without_platform_requests() {
    let host = Host::default();
    let (generation, mut requests) = host.connect();
    let mut fixed = apply_sdk(
        direct(&[], std::path::Path::new("/")).unwrap(),
        json!({
            "name":"startup-device", "manufacturer":"startup-maker", "model":"startup-model",
            "osVersion":"startup-os", "appVersion":"startup-version", "appBuildNumber":"42",
            "cpuArch":"startup-arch", "socModel":"startup-cpu",
            "android.sdkVersion":35, "android.buildTime":1700000000123i64
        })
        .as_object()
        .unwrap()
        .clone(),
        false,
        true,
    )
    .unwrap();
    fixed.total_memory = Long(123);
    let fields = FIELDS
        .iter()
        .filter(|field| !matches!(**field, "totalStorage" | "language"))
        .map(|field| (*field).to_owned())
        .chain(
            ANDROID_FIELDS
                .iter()
                .map(|field| format!("android.{field}")),
        )
        .chain(std::iter::once("android".to_owned()))
        .collect::<Vec<_>>();
    for _ in 0..2 {
        let query = collect(
            &host,
            std::path::PathBuf::from("/"),
            fields.clone(),
            fixed.clone(),
        );
        tokio::pin!(query);
        let info = tokio::select! {
            request = requests.recv() => panic!("Unexpected platform request: {request:?}"),
            result = &mut query => result.unwrap(),
        };
        assert_eq!(info.name, "startup-device");
        assert_eq!(info.manufacturer, "startup-maker");
        assert_eq!(info.model, "startup-model");
        assert_eq!(info.os_version, "startup-os");
        assert_eq!(info.app_version, "startup-version");
        assert_eq!(info.app_build_number, "42");
        assert_eq!(info.cpu_arch, "startup-arch");
        assert_eq!(info.cpu_model.as_deref(), Some("startup-cpu"));
        assert_eq!(info.total_memory.0, 123);
        let android = info.android.unwrap();
        assert_eq!(android.sdk_version, 35);
        assert_eq!(android.build_time.0.timestamp_millis(), 1700000000123);
    }
    assert!(requests.try_recv().is_err());
    host.disconnect(generation);
}

#[tokio::test]
async fn default_device_name_reuses_startup_metadata_without_a_host_request() {
    let host =
        Host::with_startup_properties(json!({"name":"startup-name"}).as_object().unwrap().clone())
            .unwrap();
    let (generation, mut requests) = host.connect();
    for _ in 0..2 {
        assert_eq!(default_device_name(&host).await.unwrap(), "startup-name");
    }
    assert!(requests.try_recv().is_err());
    host.disconnect(generation);
}

#[test]
fn fixed_information_is_ready_without_a_runtime_or_blocking_task() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let host = Host::default();
    let mut fixed = direct(&[], std::path::Path::new("/")).unwrap();
    fixed.name = "startup-name".into();
    fixed.app_version = "startup-version".into();
    let mut query = std::pin::pin!(collect(
        &host,
        std::path::PathBuf::from("/does-not-exist"),
        vec!["name".into(), "appVersion".into(), "totalMemory".into()],
        fixed,
    ));
    let Poll::Ready(result) = query.as_mut().poll(&mut Context::from_waker(Waker::noop())) else {
        panic!("Fixed information must not dispatch background work");
    };
    let info = result.unwrap();
    assert_eq!(info.name, "startup-name");
    assert_eq!(info.app_version, "startup-version");
}
