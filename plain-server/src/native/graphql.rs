use crate::graphql::executor::GraphqlExecutor;
use async_graphql::{Request, Response};
use plain_rs::events::WsEvent;
use tokio::sync::{broadcast, watch};

#[derive(Clone)]
pub struct NativeGraphqlExecutor {
    executor: GraphqlExecutor,
    events: broadcast::Sender<WsEvent>,
    client_id: String,
    stop: watch::Receiver<bool>,
}

impl NativeGraphqlExecutor {
    pub(crate) fn new(
        executor: GraphqlExecutor,
        events: broadcast::Sender<WsEvent>,
        client_id: String,
        stop: watch::Receiver<bool>,
    ) -> Self {
        Self {
            executor,
            events,
            client_id,
            stop,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<WsEvent> {
        self.events.subscribe()
    }

    pub async fn wait_stopped(&self) {
        let mut stop = self.stop.clone();
        let _ = stop.wait_for(|stopped| *stopped).await;
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub async fn execute(&self, request: Request) -> Response {
        self.executor.execute(request).await
    }
}

#[derive(Clone)]
pub struct NativeRequestScope {
    id: String,
    closed: watch::Sender<bool>,
}

impl Default for NativeRequestScope {
    fn default() -> Self {
        let (closed, _) = watch::channel(false);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            closed,
        }
    }
}

impl NativeRequestScope {
    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    pub fn close(&self) {
        self.closed.send_replace(true);
    }
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl NativeGraphqlExecutor {
    pub async fn execute_scoped(&self, request: Request, scope: NativeRequestScope) -> Response {
        let mut closed = scope.closed.subscribe();
        let rejected = || {
            Response::from_errors(vec![async_graphql::ServerError::new(
                "Native request scope closed",
                None,
            )])
        };
        if *closed.borrow() {
            return rejected();
        }
        tokio::select! {
            biased;
            _ = closed.changed() => rejected(),
            result = self.execute(request.data(scope)) => result,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/graphql.rs"]
mod tests;
