use crate::assembly::server::ServerState;
use plain_rs::events::WsEvent;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    text: String,
    sensitive: bool,
}

pub(crate) fn report(state: &ServerState, payload: &str) -> Result<(), String> {
    let value: Report = serde_json::from_str(payload).map_err(|error| error.to_string())?;
    if !crate::http::auth::public_gate::is_enabled(&state.prefs, &["CLIPBOARD"]) {
        return Ok(());
    }
    let (item, inserted) =
        plain_rs::clipboard::record(&state.db, &value.text, "", "", value.sensitive)?;
    if !inserted {
        return Ok(());
    }
    let _ = state
        .events
        .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
    if !item.sensitive {
        let time = chrono::DateTime::parse_from_rfc3339(&item.created_at)
            .map_err(|error| error.to_string())?
            .timestamp_millis();
        let _ = state.events.send(WsEvent::broadcast(
            "CLIPBOARD_CHANGED",
            serde_json::json!({"text": item.text, "sensitive": false, "time": time}).to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/services/clipboard.rs"]
mod tests;
