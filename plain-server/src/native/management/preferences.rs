use super::NativeManagement;
use crate::services::settings::{
    preferences::Preferences, security_preferences::SecurityPreferences,
};

use plain_rs::prefs::pref_keys as keys;
use serde_json::{Map, Value, json};

impl NativeManagement {
    pub fn get_prefs(&self, keys: &[String]) -> Result<Map<String, Value>, String> {
        let values = self.preferences()?;
        keys.iter().map(|key| {
            values.get(key).cloned().map(|value| (key.clone(), value))
                .ok_or_else(|| format!("Unknown preference: {key}"))
        }).collect()
    }

    pub fn get_pref(&self, key: &str) -> Result<Value, String> {
        self.state.native_client_id()?;
        if let Some(value) = plain_rs::prefs::settings::get_entry(&self.state.prefs, key)
            .map_err(|error| error.to_string())?
        {
            return Ok(value);
        }
        self.preferences()?.remove(key).ok_or_else(|| format!("Unknown preference: {key}"))
    }

    pub fn preferences(&self) -> Result<Map<String, Value>, String> {
        self.state.native_client_id()?;
        let state = &self.state;
        let prefs = &state.prefs;
        let security =
            plain_rs::prefs::security::snapshot(prefs).map_err(|error| error.to_string())?;
        let mut values =
            plain_rs::prefs::settings::values(prefs).map_err(|error| error.to_string())?;
        let version = state
            .host
            .startup_property("appVersion")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let update_info =
            crate::services::settings::updates::info(state).map_err(|error| error.to_string())?;
        let skip_version = prefs.get_or(keys::SKIP_VERSION, String::new());
        let app_update_available =
            plain_rs::updates::available(&update_info, version, &skip_version);
        let onboarding_completed: bool = state
            .store
            .get()
            .map_err(|error| error.to_string())?
            .get_json("onboarding_completed")
            .map_err(|error| error.to_string())?
            .unwrap_or(false);
        values.extend([
            (keys::PASSWORD.into(), json!(security.password)),
            (keys::PASSWORD_TYPE.into(), json!(security.password_type)),
            (
                keys::LOGIN_REQUIRE_APPROVAL.into(),
                json!(security.login_require_approval),
            ),
            (
                keys::ROTATE_URL_TOKEN_ON_RESTART.into(),
                json!(security.rotate_url_token_on_restart),
            ),
            (keys::ADB_TOKEN.into(), json!(security.adb_token)),
            (keys::URL_TOKEN.into(), json!(security.url_token)),
            (
                keys::API_PERMISSIONS.into(),
                json!(security.api_permissions),
            ),
            (keys::CLIENT_ID.into(), json!(security.client_id)),
            (keys::MDNS_HOSTNAME.into(), json!(security.mdns_hostname)),
            (
                keys::AUTO_CHECK_UPDATE.into(),
                json!(prefs.get_or(keys::AUTO_CHECK_UPDATE, true)),
            ),
            (keys::SKIP_VERSION.into(), json!(skip_version)),
            (
                keys::SIGNATURE_PUBLIC_KEY.into(),
                json!(security.signature_public_key),
            ),
            (keys::UPDATE_INFO.into(), json!(update_info)),
            (keys::APP_UPDATE_AVAILABLE.into(), json!(app_update_available)),
            (keys::ONBOARDING_COMPLETED.into(), json!(onboarding_completed)),
        ]);
        Ok(values)
    }

    pub async fn set_pref(&self, key: &str, value: serde_json::Value) -> Result<bool, String> {
        self.state.native_client_id()?;
        crate::services::settings::preferences::Preferences::from_server(&self.state)
            .set(key, value)
            .await
            .map_err(|e| e.to_string())
    }

    fn security_preferences(&self) -> Result<SecurityPreferences<'_>, String> {
        self.state.native_client_id()?;
        Ok(SecurityPreferences {
            prefs: &self.state.prefs,
            events: Some(&self.state.events),
            server: Some(&self.state),
        })
    }

    pub fn reset_password(&self) -> Result<bool, String> {
        self.security_preferences()?
            .reset_password()
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub fn reset_url_token(&self) -> Result<bool, String> {
        self.security_preferences()?
            .reset_url_token()
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub fn reset_adb_token(&self) -> Result<bool, String> {
        self.security_preferences()?
            .reset_adb_token()
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub async fn set_notification_filter_mode(&self, mode: String) -> Result<bool, String> {
        self.state.native_client_id()?;
        Preferences::from_server(&self.state)
            .action(plain_rs::prefs::settings::Action::SetNotificationMode(mode))
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn clear_notification_filter_apps(&self) -> Result<bool, String> {
        self.state.native_client_id()?;
        Preferences::from_server(&self.state)
            .action(plain_rs::prefs::settings::Action::ClearNotificationApps)
            .await
            .map_err(|error| error.to_string())
    }

    pub fn scan_history(&self) -> Result<Vec<String>, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .with_conn(|conn| -> rusqlite::Result<Vec<String>> {
                let mut query = conn.prepare("SELECT text FROM scan_history ORDER BY id DESC")?;
                query.query_map([], |row| row.get(0))?.collect()
            })
            .map_err(|error| error.to_string())
    }

    pub fn add_scan_history(&self, text: String) -> Result<bool, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .with_conn(|conn| conn.execute("INSERT INTO scan_history(text) VALUES (?1)", [&text]))
            .map_err(|error| error.to_string())?;
        Ok(true)
    }

    pub fn remove_scan_history(&self, text: String) -> Result<bool, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .with_conn(|conn| conn.execute("DELETE FROM scan_history WHERE text = ?1", [&text]))
            .map_err(|error| error.to_string())?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/native/management/preferences.rs"]
mod tests;
