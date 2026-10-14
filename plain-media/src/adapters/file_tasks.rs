use crate::events::EVENT_FILE_TASK_PROGRESS;
use crate::events::EventBus;
use anyhow::{Result, anyhow};
use plain_rs::files::tasks::Events;
pub use plain_rs::files::tasks::FileTask;
pub use plain_rs::files::tasks::FileTaskOp;
pub use plain_rs::files::tasks::FileTaskStatus;
pub use plain_rs::files::tasks::FileTaskType;
use plain_rs::files::tasks::Service;
use std::sync::{Arc, Mutex};

struct ProgressEvents;
impl Events for ProgressEvents {
    fn changed(&self, t: &FileTask) {
        EventBus::global().publish_with_cid(EVENT_FILE_TASK_PROGRESS,&t.client_id,serde_json::json!({
            "id":t.id,"type":t.kind,"title":t.title,"status":t.status,"error":t.error,
            "totalBytes":t.total_bytes,"doneBytes":t.done_bytes,"totalItems":t.total_items,"doneItems":t.done_items,"createdAt":t.created_at,"updatedAt":t.updated_at,
        }));
    }
}
static MGR: Mutex<Option<Arc<Service>>> = Mutex::new(None);
fn get_manager() -> Result<Arc<Service>> {
    let mut manager = MGR
        .lock()
        .map_err(|_| anyhow!("file task manager poisoned"))?;
    if let Some(service) = manager.as_ref() {
        return Ok(service.clone());
    }
    tokio::runtime::Handle::try_current()?;
    let service = Arc::new(Service::new(Arc::new(ProgressEvents)));
    *manager = Some(service.clone());
    Ok(service)
}
pub fn create(
    client_id: &str,
    kind: FileTaskType,
    title: &str,
    ops: Vec<FileTaskOp>,
) -> Result<FileTask> {
    get_manager()?.create(client_id, kind, title, ops)
}
pub fn create_copy_task(client_id: &str, ops: Vec<FileTaskOp>) -> Result<FileTask> {
    create(client_id, FileTaskType::Copy, "Copy files", ops)
}
pub fn create_move_task(client_id: &str, ops: Vec<FileTaskOp>) -> Result<FileTask> {
    create(client_id, FileTaskType::Move, "Move files", ops)
}
pub fn list_tasks(client_id: &str) -> Result<Vec<FileTask>> {
    get_manager()?.list(client_id)
}

#[cfg(test)]
#[path = "../../tests/unit/adapters/file_tasks.rs"]
mod tests;
