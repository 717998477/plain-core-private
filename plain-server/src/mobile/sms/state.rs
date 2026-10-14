use crate::assembly::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::db::Db;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
    Archives,
    Archive { id: String, date: String },
    Unarchive { id: String },
    Trashed,
    Trash { ids: Vec<String> },
    Restore { ids: Vec<String> },
}

pub(crate) fn execute(db: &Db, request: Request) -> anyhow::Result<Value> {
    Ok(match request {
        Request::Archives => json!({"items": db.archived_conversation_list()?}),
        Request::Archive { id, date } => {
            if id.is_empty() {
                anyhow::bail!("conversation id is empty");
            }
            let date = chrono::DateTime::parse_from_rfc3339(&date)?.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            db.archived_conversation_save(&plain_rs::db::ArchivedConversationRow {
                conversation_id: id,
                archived_at: date,
            })?;
            json!({"ok": true})
        }
        Request::Unarchive { id } => json!({"count": db.archived_conversation_delete(&id)?}),
        Request::Trashed => json!({"ids": db.trashed_message_ids()?}),
        Request::Trash { ids } => {
            let now = plain_rs::utils::dbtime::now_iso();
            let rows = ids.iter().map(|id| {
                let (message_id,is_mms)=match id.strip_prefix("mms_") { Some(id)=>(id,true),None=>(id.as_str(),false) };
                plain_rs::db::TrashedMessageRow { message_id:message_id.into(),is_mms,trashed_at:now.clone() }
            }).collect::<Vec<_>>();
            json!({"count": db.trashed_message_save_many(&rows)?})
        }
        Request::Restore { ids } => json!({"count":db.trashed_message_delete_by_ids(&ids)?}),
    })
}

pub(crate) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state.db, request) {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile/sms/state.rs"]
mod tests;
