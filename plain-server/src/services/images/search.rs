use crate::graphql::resolvers::files::types::FileSortBy;
use crate::graphql::resolvers::images::public_index::ImageSearchStatusType;
use crate::services::files::catalog::MediaCatalog;
use crate::services::images::models::Runtime;

use plain_rs::enums::DataType;
use serde_json::{Value, json};
use std::collections::HashSet;

fn enabled(models: &Runtime, text: &str) -> bool {
    !text.trim().is_empty() && models.snapshot().status.status == ImageSearchStatusType::Ready
}

fn sort_by(value: &str) -> Result<FileSortBy, String> {
    serde_json::from_value(Value::String(value.to_owned())).map_err(|error| error.to_string())
}

async fn filename_rows(
    catalog: &MediaCatalog,
    query: &str,
    offset: i32,
    limit: i32,
    sort: FileSortBy,
) -> Result<Vec<Value>, String> {
    catalog
        .rows(
            DataType::Image,
            query,
            sort,
            limit.max(0) as u32,
            offset.max(0) as u32,
        )
        .await
}

pub(crate) async fn rows(
    catalog: &MediaCatalog,
    models: &Runtime,
    text: &str,
    query: &str,
    offset: i32,
    limit: i32,
    sort: &str,
) -> Result<Vec<Value>, String> {
    if offset < 0 || limit < 0 {
        return Err("Invalid image pagination".into());
    }
    let sort = sort_by(sort)?;
    if !enabled(models, text) {
        return filename_rows(catalog, query, offset, limit, sort).await;
    }
    let semantic = models.search(text, 50).await?;
    let mut items = filename_rows(catalog, query, 0, i32::MAX, sort).await?;
    let ids = items
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect::<HashSet<_>>();
    let extra = semantic
        .into_iter()
        .map(|row| row.image_id)
        .filter(|id| !ids.contains(id.as_str()))
        .collect::<Vec<_>>();
    if !extra.is_empty() {
        let query = format!("ids:{} trash:false", extra.join(","));
        let mut found = filename_rows(catalog, &query, 0, extra.len() as i32, sort).await?;
        found.sort_by_key(|row| {
            extra
                .iter()
                .position(|id| Some(id.as_str()) == row["id"].as_str())
                .unwrap_or(usize::MAX)
        });
        items.extend(found);
    }
    Ok(items
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect())
}

pub(crate) async fn count(
    catalog: &MediaCatalog,
    models: &Runtime,
    text: &str,
    query: &str,
) -> Result<i32, String> {
    if !enabled(models, text) {
        return catalog
            .count(DataType::Image, query)
            .await
            .and_then(|count| i32::try_from(count).map_err(|error| error.to_string()));
    }
    let semantic = models.search(text, 50).await?;
    let ids = catalog.ids(DataType::Image, query, false).await?;
    let mut ids = ids.into_iter().collect::<HashSet<_>>();
    ids.extend(semantic.into_iter().map(|row| row.image_id));
    i32::try_from(ids.len()).map_err(|e| e.to_string())
}
