use crate::assembly::server::ServerState;
use crate::graphql::executor::GraphqlExecutor;
use crate::native::events::NativeEventStream;
use async_graphql::{Request, Response};
use tokio::sync::watch;

#[derive(Clone)]
pub struct NativeGraphqlExecutor {
    executor: GraphqlExecutor,
    state: ServerState,
    client_id: String,
}

impl NativeGraphqlExecutor {
    pub(crate) fn new(state: ServerState, client_id: String) -> Self {
        Self {
            executor: GraphqlExecutor::new(state.clone()),
            state,
            client_id,
        }
    }

    pub fn subscribe_events(&self, scope: NativeRequestScope) -> Result<NativeEventStream, String> {
        NativeEventStream::scoped(self.state.clone(), self.client_id.clone(), scope)
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
    pub(crate) async fn wait_closed(&self) {
        let mut closed = self.closed.subscribe();
        let _ = closed.wait_for(|closed| *closed).await;
    }

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
