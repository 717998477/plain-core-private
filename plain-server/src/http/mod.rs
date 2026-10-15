#[cfg(feature = "http_transport")]
pub mod listeners;

pub mod proxy;

pub mod routes;

pub mod websocket;

pub mod auth;

#[derive(Clone)]
pub(crate) struct ConnectionScheme(pub(crate) &'static str);
