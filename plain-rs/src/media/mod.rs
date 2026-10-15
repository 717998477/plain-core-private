//! Shared user-library domain — SQLite-backed audio playback queue /
//! playlists / play history, tags + tag relations and favorite folders.
//!
//! Ported 1:1 from the previous Go implementation's fjall implementations (which were ports of
//! plain-app's Room tables) so NAS and desktop run identical behavior:
//! both consumers call these functions against a [`crate::db::Db`]
//! SQLite file; only the media-library resolution seam
//! Metadata and playback execution are implemented per platform.

pub mod duration_cache;

pub mod actions;

pub mod images;

pub mod videos;

pub mod audio;

pub mod documents;
