pub use plain_rs::platform::linux::device_info::*;

pub async fn collect() -> anyhow::Result<DeviceInfo> {
    plain_rs::platform::linux::device_info::collect(
        crate::platform::version::version(),
        &crate::platform::version::full_version(),
    )
}
