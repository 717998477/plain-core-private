use crate::host::bridge::Host;
use serde_json::{Value, json};
use std::path::Path;

pub(crate) async fn read(
    host: &Host,
    directory: &Path,
    operation: &str,
    path: &str,
    params: Value,
) -> Result<Value, String> {
    if operation == "mime" && !path.starts_with("content://") {
        return Ok(Value::String(
            plain_rs::utils::mime::mime_from_ext(path).into(),
        ));
    }
    if operation == "animated" {
        let path = path.to_owned();
        return tokio::task::spawn_blocking(move || {
            plain_rs::files::filesystem::image_format::is_animated_image_or_svg(Path::new(&path))
        })
        .await
        .map(Value::Bool)
        .map_err(|error| error.to_string());
    }
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return host
            .call_wait(
                "systemFileResource",
                json!({"operation":operation,"path":path,"params":params}),
            )
            .await;
    }
    native(directory, operation, path, params).await
}

async fn native(
    directory: &Path,
    operation: &str,
    path: &str,
    params: Value,
) -> Result<Value, String> {
    match operation {
        "appDir" => {
            if !directory.is_absolute() {
                return Err("Absolute core directory required".into());
            }
            Ok(Value::String(
                directory.join("files").to_string_lossy().into_owned(),
            ))
        }
        "mime" => Ok(Value::String(
            plain_rs::utils::mime::mime_from_ext(path).into(),
        )),
        "remux" => Ok(Value::String(path.into())),
        "decodePng" => {
            let mime = plain_rs::utils::mime::mime_from_ext(path);
            if !mime.starts_with("image/")
                || matches!(
                    mime,
                    "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/svg+xml"
                )
            {
                return Ok(Value::Bool(false));
            }
            let output = params["output"]
                .as_str()
                .ok_or("PNG output path required")?
                .to_owned();
            let path = path.to_owned();
            tokio::task::spawn_blocking(move || -> Result<Value, String> {
                let image = match image::open(path) {
                    Ok(image) => image,
                    Err(image::ImageError::Unsupported(_)) => return Ok(Value::Bool(false)),
                    Err(error) => return Err(error.to_string()),
                };
                image
                    .save_with_format(output, image::ImageFormat::Png)
                    .map_err(|error| error.to_string())?;
                Ok(Value::Bool(true))
            })
            .await
            .map_err(|error| error.to_string())?
        }
        "probe" => {
            #[cfg(feature = "media")]
            {
                Ok(Value::String(
                    plain_media::previews::videos::probe_video_codec(path).await,
                ))
            }
            #[cfg(not(feature = "media"))]
            {
                Ok(Value::String(String::new()))
            }
        }
        "transcode" => {
            #[cfg(feature = "media")]
            {
                plain_media::previews::videos::transcode_mp4_for_browser(path)
                    .await
                    .map(|path| Value::String(path.to_string_lossy().into_owned()))
                    .map_err(|error| error.to_string())
            }
            #[cfg(not(feature = "media"))]
            {
                Ok(Value::Null)
            }
        }
        _ => Err(format!(
            "Unsupported native resource primitive: {operation}"
        )),
    }
}

pub(crate) async fn app_directory(
    host: &Host,
    directory: &Path,
) -> Result<std::path::PathBuf, String> {
    let value = read(host, directory, "appDir", "", json!({})).await?;
    let path = std::path::PathBuf::from(value.as_str().ok_or("Missing app directory")?);
    if !path.is_absolute() {
        return Err("Absolute app directory required".into());
    }
    Ok(path)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/resources.rs"]
mod tests;
