use anyhow::{Result, anyhow};
use serde_json::Value;

pub(super) fn registration(key: &[u8], timestamp: i64) -> Result<Vec<u8>> {
    plain_rs::crypto::xchacha_encrypt_raw(key, timestamp.to_string().as_bytes())
        .ok_or_else(|| anyhow!("Invalid management session key"))
}

pub(super) fn event(
    frame: &[u8],
    key: &[u8],
    stream: &mut plain_rs::protocols::websocket::Stream,
) -> Result<Option<(String, Value)>> {
    let (kind, plain) = stream
        .decode(frame, key)
        .ok_or_else(|| anyhow!("Invalid management event authentication"))?;
    let kind = kind.as_str();
    if !matches!(
        kind,
        "NOTIFICATION_CREATED"
            | "NOTIFICATION_UPDATED"
            | "NOTIFICATION_DELETED"
            | "NOTIFICATION_REFRESHED"
            | "PERMISSIONS_UPDATED"
            | "DEVICE_NAME_UPDATED"
            | "CLIPBOARD_CHANGED"
    ) {
        return Ok(None);
    }
    if kind != "CLIPBOARD_CHANGED" {
        return Ok(Some((kind.to_owned(), Value::Null)));
    }
    let data: Value = serde_json::from_slice(&plain)?;
    Ok((!data.is_null()).then(|| (kind.to_owned(), data)))
}

#[cfg(test)]
#[path = "../../../tests/unit/services/peers/management_wire.rs"]
mod tests;
