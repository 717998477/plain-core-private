use crate::graphql::resolvers::files::types::AudioFileInfo;
use crate::graphql::resolvers::files::types::DriveType;
use crate::graphql::resolvers::files::types::File;
use crate::graphql::resolvers::files::types::FileInfo;
use crate::graphql::resolvers::files::types::FileSortBy;
use crate::graphql::resolvers::files::types::ImageFileInfo;
use crate::graphql::resolvers::files::types::Location;
use crate::graphql::resolvers::files::types::MediaFileInfo;
use crate::graphql::resolvers::files::types::Mount;
use crate::graphql::resolvers::files::types::VideoFileInfo;
use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use crate::graphql::types::facts::flag;
use crate::graphql::types::facts::id;
use crate::graphql::types::facts::integer;
use crate::graphql::types::facts::list;
use crate::graphql::types::facts::rows;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use crate::http::auth::public_gate;
use crate::services::files::browse as file_reads;
use async_graphql::{Context, Enum, Object};
use chrono::{DateTime, Utc};
use plain_rs::files::filesystem::SortBy;
use plain_rs::files::filesystem::browse::Request;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

#[derive(Default)]
pub struct FilesQuery;

#[Object]
impl FilesQuery {
    async fn indexed_files(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
        sort_by: FileSortBy,
    ) -> async_graphql::Result<Vec<File>> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(Vec::new());
        }
        let items = ctx
            .data::<Arc<crate::services::files::platform::FilePlatform>>()?
            .indexed_files(query, offset, limit, sort_by, false)
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(rows(&items, file))
    }
    async fn indexed_file_count(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<Long> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(Long(0));
        }
        let count = ctx
            .data::<Arc<crate::services::files::platform::FilePlatform>>()?
            .indexed_files(query, 0, 0, FileSortBy::NameAsc, true)
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(Long(count.as_i64().ok_or("invalid indexed file count")?))
    }

    async fn browse_files(
        &self,
        ctx: &Context<'_>,
        root: String,
        query: String,
        text: Option<String>,
        show_hidden: Option<bool>,
        sort_by: FileSortBy,
        offset: i32,
        limit: i32,
        #[graphql(default)] count_only: bool,
    ) -> async_graphql::Result<FilePage> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let page = file_reads::execute(
            ctx.data::<Arc<crate::services::files::platform::FilePlatform>>()?,
            Request {
                root,
                query,
                text,
                show_hidden,
                sort_by: sort_by.into(),
                offset: usize::try_from(offset)?,
                limit: Some(usize::try_from(limit)?),
                count_only,
            },
        )
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(FilePage {
            count: Long(page["count"].as_i64().ok_or("invalid file count")?),
            items: list(&page, "items", file),
        })
    }
    /// Volumes the platform reports. The host builds these because drive
    /// enumeration is platform state; there is nothing to gate.
    async fn mounts(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Mount>> {
        let facts = host_call(ctx, crate::host::platform::SYSTEM_MOUNT_FACTS, json!({})).await?;
        Ok(rows(&facts, mount))
    }

    /// Recently-opened files. Reading them is a storage read, so this one
    /// gates — plain-app's `recentFiles` calls `checkEnabledAsync`.
    async fn recent_files(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<File>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let items = ctx
            .data::<Arc<crate::services::files::platform::FilePlatform>>()?
            .recent_files(prefs(ctx))
            .await?;
        Ok(items.iter().map(file).collect())
    }

    /// One page of a directory listing, filtered and sorted by the search
    /// DSL. `query` may carry a `parent` field to redirect the root, which
    /// is what the file manager's breadcrumb links send.
    async fn files(
        &self,
        ctx: &Context<'_>,
        root: String,
        query: String,
        offset: i32,
        limit: i32,
        sort_by: FileSortBy,
    ) -> async_graphql::Result<Vec<File>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let page = file_reads::execute(
            ctx.data::<Arc<crate::services::files::platform::FilePlatform>>()?,
            Request {
                root,
                query,
                text: None,
                show_hidden: None,
                sort_by: sort_by.into(),
                offset: offset.max(0) as usize,
                limit: Some(limit.max(0) as usize),
                count_only: false,
            },
        )
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(list(&page, "items", file))
    }

    /// Total matches for the same query, ignoring paging.
    async fn file_count(
        &self,
        ctx: &Context<'_>,
        root: String,
        query: String,
    ) -> async_graphql::Result<i32> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let page = file_reads::execute(
            ctx.data::<Arc<crate::services::files::platform::FilePlatform>>()?,
            Request {
                root,
                query,
                text: None,
                show_hidden: None,
                sort_by: SortBy::NameAsc,
                offset: 0,
                limit: None,
                count_only: true,
            },
        )
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(page["count"].as_u64().unwrap_or_default() as i32)
    }

    /// Stat plus, for media names, the platform's decoder output. A path
    /// that cannot be resolved is not an error: plain-app reports zeros so
    /// the info panel renders rather than blowing up on a stale link.
    ///
    /// The probe only runs once the stat succeeded — that stat is what the
    /// file-access authorization rides on, so probing a denied path would
    /// hand a client metadata for bytes it may not read.
    async fn file_info(
        &self,
        ctx: &Context<'_>,
        path: String,
        file_name: Option<String>,
    ) -> async_graphql::Result<FileInfo> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let record = file_reads::stat_record(
            ctx.data::<Arc<crate::services::files::platform::FilePlatform>>()?,
            &path,
        )
        .await;
        let name = file_name.unwrap_or_else(|| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let data = match &record {
            Some(_) => media_info(ctx, &path, &name).await?,
            None => None,
        };
        Ok(FileInfo {
            path,
            updated_at: record
                .as_ref()
                .map(|record| instant(record.updated_at))
                .unwrap_or_else(epoch),
            size: Long(record.map(|record| record.size).unwrap_or_default()),
            data,
        })
    }

    /// A total predicate, never an error: a blank path, `"."`, a missing
    /// file and a denied path all read as `false` so callers can use it in
    /// a filter without guarding every call.
    async fn path_exists(&self, ctx: &Context<'_>, path: String) -> async_graphql::Result<bool> {
        Ok(file_reads::stat_record(
            ctx.data::<Arc<crate::services::files::platform::FilePlatform>>()?,
            &path,
        )
        .await
        .is_some())
    }

    /// Same total-predicate semantics as `pathExists`; `null` when the path
    /// cannot be resolved.
    async fn path_kind(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<Option<PathKind>> {
        Ok(file_reads::stat_record(
            ctx.data::<Arc<crate::services::files::platform::FilePlatform>>()?,
            &path,
        )
        .await
        .map(|record| {
            if record.is_dir {
                PathKind::Dir
            } else {
                PathKind::File
            }
        }))
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
enum PathKind {
    File,
    Dir,
}

impl From<FileSortBy> for SortBy {
    fn from(sort_by: FileSortBy) -> Self {
        match sort_by {
            FileSortBy::DateAsc => SortBy::DateAsc,
            FileSortBy::DateDesc => SortBy::DateDesc,
            FileSortBy::SizeAsc => SortBy::SizeAsc,
            FileSortBy::SizeDesc => SortBy::SizeDesc,
            FileSortBy::NameAsc | FileSortBy::TakenAtDesc => SortBy::NameAsc,
            FileSortBy::NameDesc => SortBy::NameDesc,
        }
    }
}

/// The probe is chosen by file *name*, not by sniffing: the client may pass
/// a display name that differs from the on-disk one, and plain-app keys off
/// the extension either way.
async fn media_info(
    ctx: &Context<'_>,
    path: &str,
    name: &str,
) -> async_graphql::Result<Option<MediaFileInfo>> {
    let extension = Path::new(name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let method = if is_image(&extension) {
        "systemImageFileInfo"
    } else if is_video(&extension) {
        "systemVideoFileInfo"
    } else if is_audio(&extension) {
        "systemAudioFileInfo"
    } else {
        return Ok(None);
    };
    let facts = host_call(ctx, method, json!({ "path": path })).await?;
    if facts.is_null() {
        return Ok(None);
    }
    Ok(Some(match method {
        "systemImageFileInfo" => MediaFileInfo::Image(ImageFileInfo {
            width: integer(&facts, "width") as i32,
            height: integer(&facts, "height") as i32,
            location: location(&facts),
        }),
        "systemVideoFileInfo" => MediaFileInfo::Video(VideoFileInfo {
            width: integer(&facts, "width") as i32,
            height: integer(&facts, "height") as i32,
            duration_ms: Long(integer(&facts, "durationMs")),
            location: location(&facts),
        }),
        _ => MediaFileInfo::Audio(AudioFileInfo {
            duration_ms: Long(integer(&facts, "durationMs")),
            location: location(&facts),
        }),
    }))
}

fn location(value: &Value) -> Option<Location> {
    let raw = &value["location"];
    if let (Some(latitude), Some(longitude)) = (raw["latitude"].as_f64(), raw["longitude"].as_f64())
    {
        return Some(Location {
            latitude,
            longitude,
        });
    }
    let raw = value["rawLocation"].as_str()?;
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let matched = PATTERN
        .get_or_init(|| regex::Regex::new(r"([+\-]\d{1,3}\.\d{4})([+\-]\d{1,3}\.\d{4})").unwrap())
        .captures(raw)?;
    Some(Location {
        latitude: matched[1].parse().ok()?,
        longitude: matched[2].parse().ok()?,
    })
}

/// Kept in step with plain-app's `isImageFast` / `isVideoFast` /
/// `isAudioFast`, which are extension checks over the same extension sets
/// the platform decoders register.
fn is_image(extension: &str) -> bool {
    matches!(
        extension,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "svg"
    )
}

fn is_video(extension: &str) -> bool {
    matches!(
        extension,
        "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "3gp" | "ts" | "flv" | "wmv"
    )
}

fn is_audio(extension: &str) -> bool {
    matches!(
        extension,
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus" | "amr" | "wma" | "mid"
    )
}

fn file(item: &Value) -> File {
    File {
        permission: text(item, "permission"),
        media_id: item["mediaId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(|value| async_graphql::ID::from(value)),
        name: text(item, "name"),
        path: text(item, "path"),
        created_at: item["createdAt"].as_i64().map(|millis| instant(millis)),
        updated_at: instant(item["updatedAt"].as_i64().unwrap_or_default()),
        size: Long(item["size"].as_i64().unwrap_or_default()),
        is_dir: flag(item, "isDir"),
        child_count: integer(item, "childCount") as i32,
    }
}

fn mount(item: &Value) -> Mount {
    Mount {
        id: async_graphql::ID(format!("path:{}", text(item, "path"))),
        name: text(item, "name"),
        path: text(item, "path"),
        mount_point: text(item, "path"),
        fs_type: text(item, "fsType"),
        total_bytes: Long(integer(item, "totalBytes")),
        used_bytes: Long(
            integer(item, "totalBytes")
                .saturating_sub(integer(item, "freeBytes"))
                .max(0),
        ),
        free_bytes: Long(integer(item, "freeBytes")),
        remote: flag(item, "remote"),
        alias: text(item, "alias"),
        drive_type: DriveType::parse(&text(item, "driveType")),
        disk_id: text(item, "diskId"),
    }
}

fn instant(millis: i64) -> Instant {
    Instant(DateTime::<Utc>::from_timestamp_millis(millis).unwrap_or_else(epoch_datetime))
}

fn epoch() -> Instant {
    Instant(epoch_datetime())
}

fn epoch_datetime() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_default()
}

fn host<'a>(ctx: &'a Context<'_>) -> &'a Arc<Host> {
    ctx.data_unchecked::<Arc<Host>>()
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<plain_rs::prefs::Prefs> {
    ctx.data_unchecked::<Arc<plain_rs::prefs::Prefs>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    host(ctx)
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/files/public.rs"]
mod tests;

#[derive(async_graphql::SimpleObject)]
struct FilePage {
    count: Long,
    items: Vec<File>,
}
