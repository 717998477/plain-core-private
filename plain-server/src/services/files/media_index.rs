use plain_rs::{db::MediaFileMetadataRead, enums::DataType};
use std::{fs::File, io::BufReader, path::Path};

pub(crate) fn read_files(
    directory: &Path,
    cache: &Path,
    excluded_roots: &[String],
    paths: Vec<String>,
) -> Result<Vec<MediaFileMetadataRead>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    for path in &paths {
        super::platform::authorize_native(Path::new(path), false)
            .map_err(|error| error.to_string())?;
    }
    let directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    for path in paths {
        let path = Path::new(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.is_file() {
            return Err("Regular completed file required".into());
        }
        let name = path
            .to_str()
            .ok_or("Invalid completed file path encoding")?;
        if plain_rs::media::catalog_paths::is_media_excluded_at(
            name,
            &directory,
            cache,
            excluded_roots,
        ) {
            continue;
        }
        let kind = plain_rs::media::catalog_paths::infer_type(name);
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
        let read = plain_rs::media::file_metadata::read(&path, data_type, |reader: File| {
            image::ImageReader::new(BufReader::new(reader))
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()
        })
        .map_err(|error| error.to_string())?;
        if data_type == DataType::Doc && read.metadata.size_bytes == 0 {
            continue;
        }
        files.push(read);
    }
    Ok(files)
}
