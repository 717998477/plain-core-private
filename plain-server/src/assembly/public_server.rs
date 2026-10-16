use super::server::{PublicServer, ServerState};
use tokio::sync::watch;

impl ServerState {
    pub(crate) async fn start_public_locked(
        &self,
        http: u16,
        https: u16,
        cert: Vec<u8>,
        key: Vec<u8>,
    ) -> Result<(u16, u16), String> {
        self.native_client_id()?;
        let mut guard = self.public_server.lock().await;
        if guard.is_some() {
            return Err("Public HTTP server is already running".into());
        }
        let (stop, receiver) = watch::channel(false);
        let router = crate::http::routes::public::build_router(self.clone(), receiver.clone());
        let listeners =
            crate::http::listeners::HttpListeners::start(router, http, https, cert, key).await?;
        let ports = (listeners.http_port, listeners.https_port);
        {
            let _control = self.peer_status.control.lock().await;
            self.peer_status
                .public_active
                .store(true, std::sync::atomic::Ordering::SeqCst);
            self.peer_status.outgoing.start(self.clone());
        }
        *self.http_failure.lock().unwrap() = None;
        let generation = crate::assembly::lifecycle::next_generation();
        let failure = listeners.failures();
        let discovery =
            crate::assembly::lifecycle::PublicDiscovery::start(self.clone(), receiver.clone());
        *guard = Some(PublicServer {
            discovery,
            listeners,
            stop,
            generation,
        });
        crate::assembly::lifecycle::monitor(self.clone(), failure, receiver, generation);
        self.http_server.started(ports);
        let _ = self.events.send(plain_rs::events::WsEvent::broadcast(
            plain_rs::events::HTTP_SERVER_STARTED,
            serde_json::json!({"generation":generation,"httpPort":ports.0,"httpsPort":ports.1})
                .to_string(),
        ));
        Ok(ports)
    }
    pub(crate) async fn configured_public_identity(
        &self,
    ) -> Result<(u16, u16, Vec<u8>, Vec<u8>), String> {
        let settings =
            plain_rs::prefs::user::snapshot(&self.prefs).map_err(|error| error.to_string())?;
        let http = u16::try_from(settings.http_port).map_err(|error| error.to_string())?;
        let https = u16::try_from(settings.https_port).map_err(|error| error.to_string())?;
        let identity = self
            .tls_identity
            .read()
            .map_err(|_| "TLS identity configuration unavailable")?
            .clone();
        let (cert, key) = tokio::task::spawn_blocking(move || identity.identity())
            .await
            .map_err(|error| error.to_string())??;
        Ok((http, https, cert, key))
    }

    pub(crate) async fn restart_public(&self) -> Result<(u16, u16), String> {
        let _core = self.shutdown.lock().await;
        self.native_client_id()?;
        let (http, https, cert, key) = self.configured_public_identity().await?;
        let receiver_running = self.dlna.is_running();
        {
            let mut server = self.public_server.lock().await.take();
            crate::assembly::lifecycle::stop_locked(self, &mut server).await;
            self.http_server.stopped();
        }
        let ports = self.start_public_locked(http, https, cert, key).await?;
        if receiver_running {
            crate::services::casting::receiver::control::execute(
                self,
                crate::services::casting::receiver::control::Request::Start { port: ports.0 },
            )
            .await?;
        }
        self.mdns
            .update_hostname(self)
            .await
            .map_err(|error| error.to_string())?;
        Ok(ports)
    }
}
