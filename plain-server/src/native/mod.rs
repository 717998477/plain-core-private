#[cfg(feature = "core")]
pub mod graphql;

#[cfg(feature = "core")]
pub mod resources;

#[cfg(feature = "core")]
pub mod resource_stream;

#[cfg(feature = "core")]
pub mod uploads;

#[cfg(feature = "core")]
pub mod archives;

#[cfg(feature = "core")]
pub mod media_catalog;

#[cfg(all(feature = "core", feature = "media"))]
pub mod audio;
