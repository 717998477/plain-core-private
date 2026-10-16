use anyhow::{Context, Result, bail};
use plain_rs::db::{MediaFileMetadata, MediaFileMetadataAvailability, MediaFileMetadataRead};
use plain_rs::enums::DataType;
use std::fs::{File, Metadata};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Read the metadata of one media file into the row the catalog stores.
///
/// `data_type` must be one the media index covers (audio / video / image /
/// doc, as `DataType::media_type_str` names them).
pub fn read_file_metadata(path: &Path, data_type: DataType) -> Result<MediaFileMetadata> {
    Ok(read_file_metadata_with_availability(path, data_type)?.metadata)
}

pub fn read_file_metadata_with_availability(
    path: &Path,
    data_type: DataType,
) -> Result<MediaFileMetadataRead> {
    path.to_str().context("media path is not valid UTF-8")?;
    let kind = data_type
        .media_type_str()
        .with_context(|| format!("{data_type:?} is not a media data type"))?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("cannot resolve media path {}", path.display()))?;
    let meta = std::fs::metadata(&canonical)
        .with_context(|| format!("cannot stat {}", canonical.display()))?;
    if !meta.is_file() {
        bail!("{} is not a regular file", canonical.display());
    }
    let path_str = canonical
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", canonical.display()))?
        .to_string();
    let bucket_id = canonical
        .parent()
        .and_then(|p| p.to_str())
        .with_context(|| format!("{path_str} has no parent directory"))?
        .to_string();
    let size_bytes = i64::try_from(meta.len())
        .with_context(|| format!("{path_str} is larger than the row can store"))?;
    let modified = meta
        .modified()
        .with_context(|| format!("{path_str} has no readable modified time"))?;

    let probed = super::probe_media(&path_str, kind);
    let duration_ms = probed.duration_ms;
    let (width, height, rotation, taken_at) = image_facts(&canonical, kind);
    let available = MediaFileMetadataAvailability {
        tags: kind != "audio" || probed.parsed,
        duration: !matches!(kind, "audio" | "video") || probed.duration_available,
        dimensions: width > 0 && height > 0,
        rotation: rotation.is_some(),
        taken_at: taken_at.is_some(),
        cover_image_id: false,
    };
    Ok(MediaFileMetadataRead {
        available,
        metadata: MediaFileMetadata {
            data_type,
            title: if probed.title.is_empty() {
                file_stem(&path_str)
            } else {
                probed.title
            },
            artist: probed.artist,
            path: path_str,
            bucket_id,
            size_bytes,
            duration_ms,
            cover_image_id: None,
            width,
            height,
            rotation: rotation.unwrap_or_default(),
            taken_at,
            created_at: created_at_of(birth_time(&meta), modified),
            updated_at: row_timestamp(modified),
        },
    })
}

/// File name without its extension, the title fallback a tagless file has
/// no better answer for.
fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

/// UTC timestamp in the row format the rest of the database uses: the
/// catalog sorts `created_at`, `updated_at` and `taken_at` by their text
/// value, so the fixed-width `...Z` rendering of `dbtime` is what keeps
/// lexicographic order equal to chronological order inside a column.
fn row_timestamp(at: SystemTime) -> String {
    let millis = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default();
    plain_rs::utils::dbtime::iso_from_unix_millis(millis)
}

/// Creation time, falling back to modified time on the filesystems that
/// record no birth time.
fn created_at_of(birth: Option<SystemTime>, modified: SystemTime) -> String {
    birth.map_or_else(|| row_timestamp(modified), row_timestamp)
}

fn birth_unix_secs(meta: &Metadata) -> Option<i64> {
    meta.created()
        .ok()
        .and_then(|created| created.duration_since(SystemTime::UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .filter(|seconds| *seconds > 0)
}

fn birth_time(meta: &Metadata) -> Option<SystemTime> {
    birth_unix_secs(meta).map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64))
}

fn image_facts(path: &Path, kind: &str) -> (i32, i32, Option<i32>, Option<String>) {
    if kind != "image" {
        return (0, 0, None, None);
    }
    File::open(path)
        .map(image_reader)
        .unwrap_or((0, 0, None, None))
}

fn image_reader(mut reader: impl Read + Seek) -> (i32, i32, Option<i32>, Option<String>) {
    let mut head = Vec::new();
    if reader
        .by_ref()
        .take(128 * 1024)
        .read_to_end(&mut head)
        .is_err()
    {
        return (0, 0, None, None);
    }
    let exif = plain_rs::utils::image_exif::read(&mut reader)
        .ok()
        .flatten();
    let metadata = plain_rs::utils::image_metadata::read(exif.as_deref().unwrap_or(&head));
    let dimensions = plain_rs::utils::image_dimensions::dimensions(&head)
        .or_else(|| {
            exif.as_deref()
                .and_then(plain_rs::utils::image_dimensions::dimensions)
        })
        .filter(|(width, height)| *width > 0 && *height > 0)
        .or_else(|| {
            reader.seek(SeekFrom::Start(0)).ok()?;
            let (width, height) = image::ImageReader::new(BufReader::new(reader))
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()?;
            Some((i32::try_from(width).ok()?, i32::try_from(height).ok()?))
        });
    let (width, height) = dimensions.unwrap_or((0, 0));
    (
        width,
        height,
        metadata.orientation.map(|_| metadata.rotation),
        metadata.taken_at(),
    )
}

#[cfg(test)]
#[path = "../../tests/unit/metadata/file_facts.rs"]
mod tests;
