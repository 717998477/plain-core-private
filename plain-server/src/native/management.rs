use crate::assembly::server::ServerState;
use plain_rs::events::WsEvent;
use serde::Serialize;
use serde_json::json;

pub use crate::services::http_server::HttpServerState;
pub use crate::services::settings::update_download::AppUpdateDownload;
pub use crate::services::settings::updates::AppUpdateStatus;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSession {
    pub client_id: String,
    pub token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSystemSettings {
    #[serde(flatten)]
    state: plain_rs::prefs::system::SystemState,
    app_update_available: bool,
}

#[derive(Clone)]
pub struct NativeManagement {
    state: ServerState,
}

impl NativeManagement {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub fn system_settings(&self) -> Result<NativeSystemSettings, String> {
        self.state.native_client_id()?;
        let state = plain_rs::prefs::system::snapshot(&self.state.prefs)
            .map_err(|error| error.to_string())?;
        let version = self
            .state
            .host
            .startup_property("appVersion")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let app_update_available = plain_rs::updates::available(&state.update_info, version);
        Ok(NativeSystemSettings {
            state,
            app_update_available,
        })
    }

    pub fn user_settings(&self) -> Result<plain_rs::prefs::user::UserSettings, String> {
        self.state.native_client_id()?;
        plain_rs::prefs::user::snapshot(&self.state.prefs).map_err(|error| error.to_string())
    }

    pub fn system_preferences(&self) -> Result<serde_json::Value, String> {
        self.state.native_client_id()?;
        Ok(serde_json::Value::Object(
            plain_rs::prefs::system::public_entries(&self.state.prefs),
        ))
    }

    pub async fn toggle_notification_app(&self, package_name: String) -> Result<bool, String> {
        self.state.native_client_id()?;
        crate::services::settings::user_preferences::UserPreferences {
            prefs: &self.state.prefs,
            events: Some(&self.state.events),
            server: Some(&self.state),
        }
        .action(plain_rs::prefs::user::Action::ToggleNotificationApp(
            package_name,
        ))
        .await
        .map_err(|error| error.to_string())
    }

    pub async fn restart_http_server(
        &self,
        http_port: Option<u16>,
        https_port: Option<u16>,
    ) -> Result<HttpServerState, String> {
        self.state.native_client_id()?;
        let receipt = crate::services::http_server::restart(&self.state, http_port, https_port)?;
        let mut stop = self.state.stop.clone();
        tokio::select! {
            biased;
            _ = stop.wait_for(|stopped| *stopped) => Err("Rust core stopped".into()),
            result = self.state.http_server.wait_finished(receipt.revision) => result,
        }
    }

    pub async fn regenerate_tls_certificate(&self) -> Result<Vec<u8>, String> {
        self.certificate(json!({"action":"generate"})).await
    }

    pub async fn import_tls_certificate_pem(
        &self,
        certificate_pem: String,
        private_key_pem: String,
    ) -> Result<Vec<u8>, String> {
        self.certificate(json!({"action":"import","certificatePem":certificate_pem,"privateKeyPem":private_key_pem})).await
    }

    pub async fn import_tls_certificate_pkcs12(
        &self,
        data: Vec<u8>,
        password: String,
    ) -> Result<Vec<u8>, String> {
        self.certificate(json!({"action":"importPkcs12","data":plain_rs::utils::base64::base64_encode(&data),"password":password})).await
    }

    async fn certificate(&self, command: serde_json::Value) -> Result<Vec<u8>, String> {
        self.state.native_client_id()?;
        let signature = crate::services::tls_identity::server_action(&self.state, command).await?;
        self.content_changed();
        Ok(signature)
    }

    pub fn create_session(&self, name: String) -> Result<NativeSession, String> {
        self.state.native_client_id()?;
        let session = crate::services::sessions::create(&self.state, name)
            .map_err(|error| error.to_string())?;
        self.content_changed();
        Ok(NativeSession {
            client_id: session.client_id,
            token: session.token,
        })
    }

    pub async fn confirm_web_login(
        &self,
        request_id: String,
        allowed: bool,
    ) -> Result<bool, String> {
        self.state.native_client_id()?;
        crate::http::auth::ws_login::Confirmation::new(self.state.clone())
            .decide(&request_id, allowed)
            .await?;
        Ok(true)
    }

    pub async fn stage_app_restore(&self, path: String) -> Result<bool, String> {
        self.state.native_client_id()?;
        let path = self
            .state
            .file_platform
            .resolve_reference(&path)
            .map_err(|error| error.to_string())?;
        crate::services::backups::restore::stage(
            path,
            self.state.directory.clone(),
            self.state.cache.root().to_owned(),
            self.state.bridge.clone(),
            self.state.stop.clone(),
        )
        .await?;
        Ok(true)
    }

    pub async fn export_app_backup(&self) -> Result<String, String> {
        self.state.native_client_id()?;
        crate::services::backups::export(
            self.state.db.clone(),
            self.state.prefs.clone(),
            self.state.store.clone(),
            self.state.directory.clone(),
            self.state.cache.root().to_owned(),
            self.state.stop.clone(),
        )
        .await
    }

    pub async fn create_crash_report(&self, report: String) -> Result<String, String> {
        self.state.native_client_id()?;
        crate::services::crash_reports::write(&self.state, report).await
    }

    pub fn verify_adb_token(&self, token: String) -> Result<bool, String> {
        self.state.native_client_id()?;
        plain_rs::prefs::system::verify_adb_token(&self.state.prefs, &token)
            .map_err(|error| error.to_string())
    }

    pub async fn start_app_update_download(&self) -> Result<AppUpdateDownload, String> {
        self.state.native_client_id()?;
        let result = crate::services::settings::update_download::execute(
            &self.state,
            crate::services::settings::update_download::Command::Start,
        )
        .await
        .map_err(|error| error.to_string())?;
        self.content_changed();
        Ok(result)
    }

    pub async fn cancel_app_update_download(&self) -> Result<AppUpdateDownload, String> {
        self.state.native_client_id()?;
        let result = crate::services::settings::update_download::execute(
            &self.state,
            crate::services::settings::update_download::Command::Cancel,
        )
        .await
        .map_err(|error| error.to_string())?;
        self.content_changed();
        Ok(result)
    }

    pub async fn check_app_update(&self, automatic: bool) -> Result<AppUpdateStatus, String> {
        self.state.native_client_id()?;
        crate::services::settings::updates::check(&self.state, automatic)
            .await
            .map_err(|error| error.to_string())
    }

    fn content_changed(&self) {
        let _ = self
            .state
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/management.rs"]
mod tests;
