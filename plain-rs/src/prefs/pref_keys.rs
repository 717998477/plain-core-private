pub const HTTP_PORT: &str = "http_port";
pub const HTTPS_PORT: &str = "https_port";
pub const DARK_THEME: &str = "dark_theme";
pub const AMOLED_DARK_THEME: &str = "amoled_dark_theme";
pub const PDF_FOLLOW_DARK_THEME: &str = "pdf_follow_dark_theme";
pub const KEEP_AWAKE: &str = "keep_awake";
pub const LOCALE: &str = "locale";
pub const BACKGROUND_SERVICE_ENABLED: &str = "background_service_enabled";
pub const DESKTOP_ACCESS: &str = "desktop_access";
pub const DLNA: &str = "dlna";
pub const DEVELOPER_MODE: &str = "developer_mode";
pub const ALLOW_ANY_HOST: &str = "allow_any_host";
pub const DEVICE_NAME: &str = "device_name";
pub const HTTPS: &str = "https";
pub const WEB_ADDRESS_BAR_EXPANDED: &str = "web_address_bar_expanded";
pub const SCREEN_MIRROR_QUALITY: &str = "screen_mirror_quality";
pub const AUDIO_PLAY_MODE: &str = "audio_play_mode";
pub const AUDIO_PLAYBACK_SPEED: &str = "audio_playback_speed";
pub const IMAGE_GRID_CELLS_PER_ROW: &str = "image_grid_cells_per_row";
pub const VIDEO_GRID_CELLS_PER_ROW: &str = "video_grid_cells_per_row";
pub const SHOW_HIDDEN_FILES: &str = "show_hidden_files";
pub const FEED_AUTO_REFRESH: &str = "feed_auto_refresh";
pub const FEED_AUTO_REFRESH_INTERVAL: &str = "feed_auto_refresh_interval";
pub const FEED_AUTO_REFRESH_ONLY_WIFI: &str = "feed_auto_refresh_only_wifi";
pub const FEED_FONT_SCALE: &str = "feed_font_scale";
pub const EDITOR_WRAP_CONTENT: &str = "editor_wrap_content";
pub const EDITOR_FONT_SIZE: &str = "editor_font_size";
pub const EDITOR_STATUS_BAR: &str = "editor_status_bar";
pub const NEARBY_DISCOVERABLE: &str = "nearby_discoverable";
pub const AI_IMAGE_SEARCH_ENABLED: &str = "ai_image_search_enabled";
pub const NOTIFICATION_FILTER: &str = "notification_filter";
pub const POMODORO_SETTINGS: &str = "pomodoro_settings";
pub const APP_HOME_FEATURES: &str = "app_home_features";
pub const LAUNCHER_SHORTCUTS: &str = "launcher_shortcuts";
pub const AUDIO_SORT_BY: &str = "audio_sort_by";
pub const VIDEO_SORT_BY: &str = "video_sort_by";
pub const IMAGE_SORT_BY: &str = "image_sort_by";
pub const DOC_SORT_BY: &str = "doc_sort_by";
pub const FILE_SORT_BY: &str = "file_sort_by";
pub const PACKAGE_SORT_BY: &str = "pkg_sort_by";

pub const PASSWORD: &str = "password";
pub const PASSWORD_TYPE: &str = "password_type";
pub const LOGIN_REQUIRE_APPROVAL: &str = "login_require_approval";
pub const ROTATE_URL_TOKEN_ON_RESTART: &str = "rotate_url_token_on_restart";
pub const ADB_TOKEN: &str = "adb_token";
pub const URL_TOKEN: &str = "url_token";
pub const MASTER_SECRET: &str = "master_secret";
pub const API_PERMISSIONS: &str = "api_permissions";
pub const CLIENT_ID: &str = "client_id";
pub const MDNS_HOSTNAME: &str = "mdns_hostname";
pub const SIGNATURE_KEY_PAIR: &str = "signature_key_pair";
pub const DEV_TOKEN: &str = "dev_token";
pub const LOG_LEVEL: &str = "log_level";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefAccess {
    Default,
    LocalOnly,
    RustOnly,
}
pub const PREF_KEYS: &[(&str, PrefAccess)] = &[
    ("dlna", PrefAccess::Default),
    ("device_name", PrefAccess::Default),
    ("feed_auto_refresh", PrefAccess::Default),
    ("feed_auto_refresh_interval", PrefAccess::Default),
    ("feed_auto_refresh_only_wifi", PrefAccess::Default),
    ("ai_image_search_enabled", PrefAccess::Default),
    ("ai_image_search_model", PrefAccess::Default),
    ("dlna_allowed_senders", PrefAccess::Default),
    ("dlna_denied_senders", PrefAccess::Default),
    ("clipboard_directions", PrefAccess::Default),
    ("media_source_dirs", PrefAccess::Default),
    ("samba_settings", PrefAccess::Default),
    ("dark_theme", PrefAccess::Default),
    ("amoled_dark_theme", PrefAccess::Default),
    ("pdf_follow_dark_theme", PrefAccess::Default),
    ("locale", PrefAccess::Default),
    ("show_hidden_files", PrefAccess::Default),
    ("feed_font_scale", PrefAccess::Default),
    ("editor_wrap_content", PrefAccess::Default),
    ("editor_font_size", PrefAccess::Default),
    ("editor_status_bar", PrefAccess::Default),
    ("desktop_home_features", PrefAccess::Default),
    ("audio_sort_by", PrefAccess::Default),
    ("video_sort_by", PrefAccess::Default),
    ("image_sort_by", PrefAccess::Default),
    ("doc_sort_by", PrefAccess::Default),
    ("file_sort_by", PrefAccess::Default),
    ("pkg_sort_by", PrefAccess::Default),
    ("audio_play_mode", PrefAccess::Default),
    ("audio_playback_speed", PrefAccess::Default),
    ("pomodoro_settings", PrefAccess::Default),
    ("screen_mirror_quality", PrefAccess::Default),
    ("volume_alias", PrefAccess::Default),
    ("http_port", PrefAccess::LocalOnly),
    ("https_port", PrefAccess::LocalOnly),
    ("keep_awake", PrefAccess::LocalOnly),
    ("background_service_enabled", PrefAccess::LocalOnly),
    ("desktop_access", PrefAccess::LocalOnly),
    ("developer_mode", PrefAccess::LocalOnly),
    ("allow_any_host", PrefAccess::LocalOnly),
    ("nearby_discoverable", PrefAccess::LocalOnly),
    ("notification_filter", PrefAccess::LocalOnly),
    ("launcher_shortcuts", PrefAccess::LocalOnly),
    ("image_grid_cells_per_row", PrefAccess::LocalOnly),
    ("video_grid_cells_per_row", PrefAccess::LocalOnly),
    ("https", PrefAccess::LocalOnly),
    ("web_address_bar_expanded", PrefAccess::LocalOnly),
    ("app_home_features", PrefAccess::LocalOnly),
    ("password", PrefAccess::LocalOnly),
    ("password_type", PrefAccess::LocalOnly),
    ("login_require_approval", PrefAccess::LocalOnly),
    ("rotate_url_token_on_restart", PrefAccess::LocalOnly),
    ("api_permissions", PrefAccess::LocalOnly),
    ("mdns_hostname", PrefAccess::LocalOnly),
    ("dev_token", PrefAccess::LocalOnly),
    ("log_level", PrefAccess::LocalOnly),
    ("auto_check_update", PrefAccess::LocalOnly),
    ("skip_version", PrefAccess::LocalOnly),
    ("client_id", PrefAccess::RustOnly),
    ("url_token", PrefAccess::RustOnly),
    ("adb_token", PrefAccess::RustOnly),
    ("master_secret", PrefAccess::RustOnly),
    ("signature_key_pair", PrefAccess::RustOnly),
 ];
pub fn access(key: &str) -> Option<PrefAccess> {
    PREF_KEYS.iter().find(|(name, _)| *name == key).map(|(_, access)| *access)
}
pub fn accessible(key: &str, local: bool) -> bool {
    matches!(access(key), Some(PrefAccess::Default)) || local && matches!(access(key), Some(PrefAccess::LocalOnly))
}
