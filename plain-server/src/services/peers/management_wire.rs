use anyhow::{Result, anyhow};
use serde_json::Value;

pub(super) fn registration(key: &[u8], timestamp: i64) -> Result<Vec<u8>> {
    plain_rs::crypto::xchacha_encrypt_raw(key, timestamp.to_string().as_bytes())
        .ok_or_else(|| anyhow!("Invalid management session key"))
}

pub(super) fn event(frame: &[u8], key: &[u8]) -> Result<Option<(String, Value)>> {
    let (kind, body) = plain_rs::protocols::websocket::decode_raw(frame)
        .ok_or_else(|| anyhow!("Invalid management event frame"))?;
    if !matches!(
        kind,
        "NOTIFICATION_CREATED"
            | "NOTIFICATION_UPDATED"
            | "NOTIFICATION_DELETED"
            | "NOTIFICATION_REFRESHED"
            | "PERMISSIONS_UPDATED"
            | "CLIPBOARD_CHANGED"
    ) {
        return Ok(None);
    }
    let plain = plain_rs::crypto::xchacha_decrypt_raw(key, body)
        .ok_or_else(|| anyhow!("Invalid management event authentication"))?;
    let data: Value = serde_json::from_slice(&plain)?;
    Ok((!data.is_null()).then(|| (kind.to_owned(), data)))
}

#[cfg(test)]
#[path = "../../../tests/unit/services/peers/management_wire.rs"]
mod tests;
