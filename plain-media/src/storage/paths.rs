use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone)]
pub struct MediaPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

static OVERRIDE: OnceLock<MediaPaths> = OnceLock::new();

pub fn set(data_dir: PathBuf, cache_dir: PathBuf) {
    let _ = OVERRIDE.set(MediaPaths {
        data_dir,
        cache_dir,
    });
}

pub fn detect() -> MediaPaths {
    OVERRIDE
        .get_or_init(|| {
            let data_dir = std::env::var("PLAIN_RS_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| default_data_dir());
            let cache_dir = plain_rs::cache_paths::system::resolve(&data_dir)
                .expect("initialize system cache directory at startup");
            MediaPaths {
                data_dir,
                cache_dir,
            }
        })
        .clone()
}

fn home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn default_data_dir() -> PathBuf {
    home().join(".plainrs").join("data")
}

/// Test seam: pin `PLAIN_RS_DATA_DIR` for the whole test binary to a
/// fixed, never-deleted directory, mirroring the previous Go implementation's
/// `AppPaths::pin_test_data_dir`. Tests that pointed this var at a
/// per-test `TempDir` poisoned the process-global media search index:
/// whichever value the env var held when the index first initialized
/// rooted the index there, and the owning test's `TempDir` cleanup then
/// deleted the index directory out from under every later test.
///
/// Public under the `test-seams` feature so plain-server's media resolver
/// tests can pin the same process-global index from their own test binary.
#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub fn pin_test_data_dir() -> PathBuf {
    static PINNED: OnceLock<PathBuf> = OnceLock::new();
    PINNED
        .get_or_init(|| {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-media-data-dir");
            std::fs::create_dir_all(&dir).expect("mkdir target/test-media-data-dir");
            // SAFETY: test-only; writes the same value on every call.
            unsafe { std::env::set_var("PLAIN_RS_DATA_DIR", &dir) };
            dir
        })
        .clone()
}

#[cfg(test)]
#[path = "../../tests/unit/storage/paths.rs"]
mod tests;
