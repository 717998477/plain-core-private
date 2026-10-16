pub(crate) fn properties() -> serde_json::Map<String, serde_json::Value> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        serde_json::Map::new()
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        serde_json::json!({
            "name":plain_rs::prefs::identity::default_device_name(),
            "deviceType":"COMPUTER",
            "updateSource":{"url":"https://api.github.com/repos/plainhub/plain-desktop/releases/latest","pageUrl":"https://github.com/plainhub/plain-desktop/releases/latest","assets":[]},
            "supportedAbis":[std::env::consts::ARCH],
            "appVersion":crate::platform::version::version(),
            "buildChannel":"GITHUB",
            "downloadsDir":"",
            "debug":cfg!(debug_assertions)
        })
        .as_object()
        .unwrap()
        .clone()
    }
}
