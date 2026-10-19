//! Shared user-library domain — SQLite-backed audio playback queue /
//! playlists / play history, tags + tag relations and favorite folders.
//!
//! Ported 1:1 from the previous Go implementation's fjall implementations (which were ports of
//! plain-app's Room tables) so NAS and desktop run identical behavior:
//! both consumers call these functions against a [`crate::db::Db`]
//! SQLite file; only the media-library resolution seam
//! Metadata and playback execution are implemented per platform.

pub mod duration_cache;
pub mod isobmff;

pub mod actions;

pub mod images;

pub mod videos;

pub mod audio;

pub mod documents;

pub mod catalog_paths;
pub mod file_metadata;
pub mod probe;

pub mod cover;

mod raw_audio;

mod asf;

mod midi;

pub mod video_info;

mod matroska;

mod avi;

mod flv;

mod h264;

mod bits;

mod transport_stream;

mod hevc;
