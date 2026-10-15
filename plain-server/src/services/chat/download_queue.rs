use crate::assembly::server::ServerState;
use plain_rs::chat::attachments::download_queue::Effect;
use plain_rs::events::WsEvent;
pub(crate) struct Runtime {
    job: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    stop: tokio::sync::watch::Sender<bool>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            job: tokio::sync::Mutex::new(None),
            stop: tokio::sync::watch::channel(false).0,
        }
    }
}
impl Runtime {
    pub(crate) async fn shutdown(&self) {
        self.stop.send_replace(true);
        if let Some(job) = self.job.lock().await.take() {
            let _ = job.await;
        }
    }
}
pub(crate) fn start(state: ServerState) {
    let runtime = state.download_runtime.clone();
    let mut changed = state.downloads.changed.subscribe();
    let mut stop = state.stop.clone();
    let mut runtime_stop = runtime.stop.subscribe();
    let job = tokio::spawn(async move {
        let mut workers: std::collections::HashMap<String, tokio::task::JoinHandle<()>> =
            std::collections::HashMap::new();
        loop {
            workers.retain(|_, job| !job.is_finished());
            if let Some(effect) = state.downloads.effect() {
                match effect {
                    Effect::Cancel { token } => {
                        if let Some(job) = workers.remove(&token) {
                            job.abort();
                            let _ = job.await;
                        }
                    }
                    Effect::Start { task, ticket } => {
                        let state = state.clone();
                        let token = task.generation.clone();
                        let job = tokio::spawn(async move {
                            let result = crate::services::peers::peer_download::run(
                                &state,
                                &task,
                                &ticket.path,
                            )
                            .await;
                            let error = result.err().map(|e| e.to_string());
                            let db = state.db.clone();
                            let id = task.message_id.clone();
                            let queue = state.downloads.clone();
                            let generation = task.generation;
                            let file_id = task.id;
                            let committed = tokio::task::spawn_blocking(move || {
                                queue.finish(&file_id, &generation, error)
                            })
                            .await;
                            if matches!(committed, Ok(Ok(true))) {
                                if let Ok(Some(chat)) =
                                    plain_rs::db::chat_store::messages::get(&db, &id)
                                {
                                    let _ = state.events.send(WsEvent::broadcast(
                                        plain_rs::chat::events::WS_MESSAGE_UPDATED,
                                        serde_json::json!([plain_rs::chat::service::chat_to_json(
                                            &chat
                                        )])
                                        .to_string(),
                                    ));
                                }
                            }
                        });
                        workers.insert(token, job);
                    }
                }
                continue;
            }
            tokio::select! {_=runtime_stop.changed()=>break,_=stop.changed()=>break,result=changed.changed()=>if result.is_err(){break}};
            let _ = state.events.send(WsEvent::broadcast(
                plain_rs::chat::events::WS_DOWNLOAD_PROGRESS,
                state.downloads.public_progress(),
            ));
        }
        for task in state.downloads.snapshot()["tasks"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(id) = task["id"].as_str() {
                let _ = state.downloads.control(id, "cancel");
            }
        }
        for (_, job) in workers {
            job.abort();
            let _ = job.await;
        }
    });
    *runtime.job.try_lock().expect("New download runtime") = Some(job);
}

#[cfg(test)]
#[path = "../../../tests/unit/services/chat/download_queue.rs"]
mod tests;
