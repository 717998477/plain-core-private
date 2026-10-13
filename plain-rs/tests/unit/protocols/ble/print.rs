use super::kotlin_uuids_source;

#[test]
fn source_is_stable() {
    let first = kotlin_uuids_source();
    assert_eq!(first, kotlin_uuids_source());
    assert!(first.contains("d8d5c4a0-8f0a-4e7d-b9e1-706c70616901"));
    assert!(first.contains("d8d5c4a0-8f0a-4e7d-b9e1-706c70616911"));
    assert!(first.contains("d8d5c4a0-8f0a-4e7d-b9e1-706c70616913"));
    assert!(first.contains("00002902-0000-1000-8000-00805f9b34fb"));
}

#[test]
#[ignore = "generator; run explicitly to regenerate BleUuids.kt"]
fn regenerate_kotlin_uuids() {
    let path = std::env::var_os("PLAIN_APP_BLE_UUIDS_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../../plain-app/shared/src/commonMain/kotlin/com/ismartcoding/plain/ble/BleUuids.kt",
            )
        });
    std::fs::write(&path, kotlin_uuids_source()).unwrap();
    eprintln!("wrote {}", path.display());
}
