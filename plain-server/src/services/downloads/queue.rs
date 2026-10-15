use crate::assembly::enums::DownloadStatus;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Clone, Debug)]
pub struct DownloadState {
    pub message_id: String,
    pub status: DownloadStatus,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
}

pub struct TaskEntry {
    pub handle: JoinHandle<()>,
    pub abort: Option<tokio::sync::oneshot::Sender<()>>,
    pub state: Arc<Mutex<DownloadState>>,
}

pub struct DownloadManager {
    pub(crate) tasks: Mutex<HashMap<String, TaskEntry>>,
}

impl DownloadManager {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }
}

pub(crate) fn manager() -> &'static DownloadManager {
    static MGR: OnceLock<DownloadManager> = OnceLock::new();
    MGR.get_or_init(DownloadManager::new)
}

pub async fn pause_download(message_id: &str) -> bool {
    let mut tasks = manager().tasks.lock().await;
    if let Some(entry) = tasks.get_mut(message_id) {
        if let Some(abort) = entry.abort.take() {
            let _ = abort.send(());
        }
        let mut state = entry.state.lock().await;
        state.status = DownloadStatus::Paused;
        return true;
    }
    false
}
