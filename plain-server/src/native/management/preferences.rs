use super::NativeManagement;
use crate::services::settings::{
    preferences::Preferences, security_preferences::SecurityPreferences,
};

impl NativeManagement {
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
