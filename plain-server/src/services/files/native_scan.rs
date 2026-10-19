use super::media_scans::MediaScans;
use plain_rs::db::Db;
use std::path::Path;

pub(crate) fn completed(
    db: &Db,
    scans: &MediaScans,
    directory: &Path,
    paths: Vec<String>,
) -> Result<(), String> {
    let policy = scans.policy()?;
    let files = super::media_index::read_files(
        directory,
        &policy.cache_directory,
        &policy.excluded_roots,
        paths,
    )?;
    let rows = scans.record_completion(|| {
        db.record_media_file_reads(&files)
            .map_err(|error| error.to_string())
    })?;
    scans.publish_catalog_rows(&rows);
    Ok(())
}
