pub mod contacts;

pub mod phone_calls;

pub mod query;

use std::sync::Arc;
pub(crate) struct Runtime {
    pub(crate) mms: Arc<crate::services::sms::mms::Runtime>,
}
impl Runtime {
    pub(crate) fn new(
        host: Arc<crate::host::bridge::Host>,
        db: Arc<plain_rs::db::Db>,
        directory: std::path::PathBuf,
        store: Arc<plain_rs::kv::LazyDb>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    ) -> Arc<Self> {
        Arc::new(Self {
            mms: crate::services::sms::mms::Runtime::new(host, db, directory, store, events),
        })
    }
}
pub mod graphql;
