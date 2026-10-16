use super::media_scans::MediaScans;
use plain_rs::{db::Db, enums::DataType};
use std::path::Path;

pub(crate) fn completed(
    db: &Db,
    scans: &MediaScans,
    directory: &Path,
    paths: Vec<String>,
) -> Result<(), String> {
    let mut files = Vec::new();
    if !paths.is_empty() {
        for path in &paths {
            super::platform::authorize_native(Path::new(path), false)
                .map_err(|error| error.to_string())?;
        }
        let policy = scans.policy()?;
        let directory = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        for path in paths {
            let path = std::path::Path::new(&path)
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !path.is_file() {
                return Err("Regular completed file required".into());
            }
            let name = path
                .to_str()
                .ok_or("Invalid completed file path encoding")?;
            if plain_media::scan::is_media_excluded_at(
                name,
                &directory,
                &policy.cache_directory,
                &policy.excluded_roots,
            ) {
                continue;
            }
            let kind = plain_media::scan::infer_type(name);
            let Some(data_type) = [
                DataType::Audio,
                DataType::Video,
                DataType::Image,
                DataType::Doc,
            ]
            .into_iter()
            .find(|data_type| data_type.media_type_str() == Some(kind)) else {
                continue;
            };
            let read = plain_media::metadata::file_facts::read_file_metadata_with_availability(
                &path, data_type,
            )
            .map_err(|error| error.to_string())?;
            if data_type == DataType::Doc && read.metadata.size_bytes == 0 {
                continue;
            }
            files.push(read);
        }
    }
    let rows = scans.record_completion(|| {
        db.record_media_file_reads(&files)
            .map_err(|error| error.to_string())
    })?;
    scans.publish_catalog_rows(&rows);
    Ok(())
}
