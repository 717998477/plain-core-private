use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessRequest {
    operation: String,
    paths: Vec<String>,
}

pub fn authorize_read(params: Value) -> Result<bool, String> {
    let request: AccessRequest =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    if request.operation != "read" || request.paths.is_empty() {
        return Err("Explicit file read operation and paths required".into());
    }
    for path in request.paths {
        let path = Path::new(&path);
        if !path.is_absolute() {
            return Err("Absolute file path required".into());
        }
        let path = path.canonicalize().map_err(|error| error.to_string())?;
        let metadata = path.metadata().map_err(|error| error.to_string())?;
        if metadata.is_file() {
            std::fs::File::open(&path).map_err(|error| error.to_string())?;
        } else if metadata.is_dir() {
            std::fs::read_dir(&path).map_err(|error| error.to_string())?;
        } else {
            return Err("Regular file or directory required".into());
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/unit/host/file_resource.rs"]
mod tests;
