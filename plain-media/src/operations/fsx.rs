//! Filesystem-related helpers that don't deserve their own crate and that
//! we'd rather not pull in a third-party dependency for. Mirrors bits of
//! `internal/pkg/pathx` and the per-op resolvers in
//! `internal/graph/files_*_api.go` + `internal/graph/helpers_local.go` from
//! the Go side.
//!
//! Inlined here: `percent_decode_path` (used in HTTP query params).
//! Inlined elsewhere: per-file-id short id, mount table parsing, etc.

use std::path::Path;

/// Errors for file ID decryption.
#[derive(Debug)]
pub enum FileIdError {
    InvalidId,
    Forbidden,
}

/// Decrypt an encrypted file ID to a filesystem path.
/// Mirrors Go `fs.PathFromFileID` in `internal/fs/file_id.go`:
///   1. Base64-decode the id (replacing ' ' back to '+' for URL safety).
///   2. Load the URL token from the preferences.
///   3. Base64-decode the token to get the 32-byte XChaCha20-Poly1305 key.
///   4. Decrypt the ciphertext → plaintext file path.
pub fn path_from_file_id(id: &str, prefs: &plain_rs::prefs::Prefs) -> Result<String, FileIdError> {
    let id = id.trim();
    if id.is_empty() {
        log::debug!("[path_from_file_id] empty id");
        return Err(FileIdError::InvalidId);
    }
    // Some URL parsers turn '+' into ' ' in query strings.
    let id = id.replace(' ', "+");

    let ciphertext = base64_decode(&id).ok_or_else(|| {
        log::debug!("[path_from_file_id] base64 decode of id failed");
        FileIdError::Forbidden
    })?;
    log::debug!("[path_from_file_id] ciphertext len: {}", ciphertext.len());

    let token = prefs
        .get::<String>(plain_rs::prefs::pref_keys::URL_TOKEN)
        .ok()
        .flatten()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            log::debug!("[path_from_file_id] no url_token found in prefs");
            FileIdError::Forbidden
        })?;

    let key = base64_decode(&token).ok_or_else(|| {
        log::debug!("[path_from_file_id] base64 decode of token failed");
        FileIdError::Forbidden
    })?;
    log::debug!("[path_from_file_id] key len: {} (expected 32)", key.len());

    let plain = plain_rs::crypto::xchacha_decrypt_raw(&key, &ciphertext).ok_or_else(|| {
        log::debug!(
            "[path_from_file_id] XChaCha20-Poly1305 decrypt failed, key_len={}, ct_len={}",
            key.len(),
            ciphertext.len()
        );
        FileIdError::Forbidden
    })?;
    if plain.is_empty() {
        log::debug!("[path_from_file_id] decrypted to empty");
        return Err(FileIdError::Forbidden);
    }
    let path = String::from_utf8(plain).map_err(|e| {
        log::debug!("[path_from_file_id] utf8 decode failed: {}", e);
        FileIdError::Forbidden
    })?;
    let path = extract_path(&path);
    Ok(path)
}

/// plain-app FileServer contract: media-item ids encrypt a JSON envelope
/// `{"path":…,"mediaId":…}` (web `getFileId` mints it whenever the item has
/// a mediaId), while plain file ids encrypt the bare path. Accept both.
fn extract_path(plain: &str) -> String {
    let trimmed = plain.trim();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                return p.to_string();
            }
        }
    }
    plain.to_string()
}

/// Minimal base64 standard decoding (matches Go `base64.StdEncoding`).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in s.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Decode a percent-encoded path/URL component into a UTF-8 `String`.
///
/// Unlike `percent_encoding::percent_decode_str` we **only** support the
/// `%XX` syntax — no `+`-as-space handling, because we never feed query
/// strings into here (query strings are still parsed with the `url`
/// crate, which already decodes them). The intent is purely to undo the
/// `axum::extract::Path<String>` double-encoding for path segments that
/// contain `%` or non-ASCII characters.
///
/// On an invalid `%XX` triplet the bytes are passed through unchanged
/// (matching `decode_utf8_lossy` semantics on the upstream crate) so a
/// malformed URL doesn't bring the whole handler down.
pub fn percent_decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Map a file extension to an IANA MIME type. Falls back to
/// `application/octet-stream` for anything we don't recognise.
///
/// We don't try to be comprehensive — the Go side uses Go's `mime`
/// package which carries ~1000 entries, but for the server front-end the
/// ~50 entries below cover 99% of served files. Adding the rest is
/// trivial when the need arises.
pub fn guess_mime(path: &std::path::Path) -> String {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mime = plain_rs::utils::mime::mime_from_ext(filename);
    if mime.starts_with("text/") {
        format!("{mime}; charset=utf-8")
    } else {
        mime.to_string()
    }
}

pub use plain_rs::files::filesystem::image_format::is_animated_image_or_svg;

// ---------------------------------------------------------------------------
// File operations
//
// The functions below are 1:1 ports of the resolvers in
// `internal/graph/files_*_api.go` and the helpers in
// `internal/graph/helpers_local.go` + `helpers/files_helper.go` from the
// Go side. They are async because every caller in the GraphQL resolvers
// is `async fn` and we want the syscall work to run on tokio's blocking
// pool (via `tokio::fs::File`) rather than blocking the reactor.
// ---------------------------------------------------------------------------

pub use plain_rs::files::filesystem::*;

#[cfg(test)]
use plain_rs::files::filesystem::count_dir_entries_fast;

#[cfg(test)]
#[path = "../../tests/unit/operations/fsx.rs"]
mod tests;
