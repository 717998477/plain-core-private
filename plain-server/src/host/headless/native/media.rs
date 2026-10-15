use serde_json::Value;
use std::path::Path;

pub fn media_info(path: &str, kind: plain_rs::enums::DataType) -> Value {
    if kind == plain_rs::enums::DataType::Image {
        let Ok(bytes) = std::fs::read(path) else {
            return Value::Null;
        };
        let Some((width, height)) = plain_rs::utils::image_dimensions::dimensions(&bytes) else {
            return Value::Null;
        };
        let location = plain_rs::utils::image_dimensions::exif_gps(&bytes).map(
            |(latitude, longitude)| serde_json::json!({"latitude":latitude,"longitude":longitude}),
        );
        return serde_json::json!({"width":width,"height":height,"location":location});
    }
    match plain_media::metadata::file_facts::read_file_metadata(Path::new(path), kind) {
        Ok(facts) => {
            serde_json::json!({"width":facts.width,"height":facts.height,"durationMs":facts.duration_ms})
        }
        Err(_) => Value::Null,
    }
}
