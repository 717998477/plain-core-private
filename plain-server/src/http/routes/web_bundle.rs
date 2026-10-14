use std::sync::Arc;

/// Read-only access to a frontend bundle that is compiled into the executable
/// rather than unpacked from a directory at startup.
pub trait WebBundle: Send + Sync {
    /// The bytes at `relative`, a `/`-separated path from the bundle root.
    fn read(&self, relative: &str) -> Option<&'static [u8]>;
}

pub type SharedWebBundle = Arc<dyn WebBundle>;

#[cfg(test)]
#[path = "../../../tests/unit/http/routes/web_bundle.rs"]
mod tests;
