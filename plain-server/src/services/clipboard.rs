use crate::assembly::server::ServerState;
use plain_rs::events::WsEvent;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    text: String,
    sensitive: bool,
}

pub(crate) fn record_history(
    db: &plain_rs::db::Db,
    events: &tokio::sync::broadcast::Sender<WsEvent>,
    text: &str,
    source: &str,
    sensitive: bool,
) -> Result<(plain_rs::db::ClipboardRow, bool), String> {
    let result = plain_rs::clipboard::record(db, text, source, "", sensitive)?;
    if result.1 {
        let _ = events.send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
    }
    Ok(result)
}

pub(crate) fn report(state: &ServerState, payload: &str) -> Result<(), String> {
    let value: Report = serde_json::from_str(payload).map_err(|error| error.to_string())?;
    if !crate::http::auth::public_gate::is_enabled(&state.prefs, &["CLIPBOARD"]) {
        return Ok(());
    }
    let (item, inserted) =
        record_history(&state.db, &state.events, &value.text, "", value.sensitive)?;
    if !inserted {
        return Ok(());
    }
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
