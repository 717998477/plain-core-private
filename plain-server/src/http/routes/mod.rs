#[cfg(feature = "core")]
pub mod cors_host;

#[cfg(feature = "core")]
pub mod static_host;

#[cfg(feature = "core")]
pub mod web_bundle;

#[cfg(feature = "core")]
pub mod graphql;

#[cfg(feature = "core")]
pub mod zip;

#[cfg(feature = "core")]
pub mod files;

#[cfg(feature = "core")]
pub mod discovery;

#[cfg(feature = "core")]
pub(crate) mod public;
