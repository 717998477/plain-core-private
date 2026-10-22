use super::NativeManagement;
use crate::services::discovery::mdns_runtime::{self, Request};
use serde_json::Value;

impl NativeManagement {
    pub async fn acquire_mdns_debug_session(&self, token: String) -> Result<Value, String> {
        self.state.native_client_id()?;
        mdns_runtime::control(&self.state, Request::DebugStart { token })
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn release_mdns_debug_session(&self, token: String) -> Result<Value, String> {
        self.state.native_client_id()?;
        mdns_runtime::control(&self.state, Request::DebugStop { token })
            .await
            .map_err(|error| error.to_string())
    }
}
