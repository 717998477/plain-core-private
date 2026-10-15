#[cfg(feature = "core")]
pub mod enums;

pub mod events;

// Identity lives with the preferences engine (`prefs::identity`).
pub use plain_rs::prefs::identity::AppIdentity;
pub use plain_rs::prefs::identity::default_device_name;
pub use plain_rs::prefs::identity::generate_identity;

#[cfg(all(feature = "core", feature = "http_transport"))]
pub mod lifecycle;

#[cfg(feature = "core")]
pub mod server;

#[cfg(feature = "headless")]
pub mod headless;

#[cfg(feature = "core")]
pub mod services;

#[cfg(feature = "core")]
pub(crate) mod core_dependencies;
