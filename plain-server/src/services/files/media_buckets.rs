use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ItemFact {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) size: i64,
    path: String,
    sort_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bucket {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) item_count: u64,
    pub(crate) size: i64,
    pub(crate) top_items: Vec<String>,
    sort_name: String,
}

pub(crate) fn aggregate(data_type: &str, items: Vec<ItemFact>) -> anyhow::Result<Vec<Bucket>> {
    anyhow::ensure!(
        matches!(data_type, "IMAGE" | "VIDEO" | "AUDIO" | "DOC"),
        "unsupported media bucket type"
    );
    let mut positions = HashMap::<String, usize>::new();
    let mut buckets = Vec::<Bucket>::new();
    for item in items {
        anyhow::ensure!(item.size >= 0, "invalid media bucket item size");
        if let Some(&index) = positions.get(&item.id) {
            let bucket = &mut buckets[index];
            anyhow::ensure!(bucket.name == item.name, "inconsistent media bucket name");
            bucket.item_count = bucket
                .item_count
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("media bucket item count overflow"))?;
            bucket.size = bucket
                .size
                .checked_add(item.size)
                .ok_or_else(|| anyhow::anyhow!("media bucket size overflow"))?;
            if bucket.top_items.len() < 4 {
                bucket.top_items.push(item.path);
            }
        } else {
            positions.insert(item.id.clone(), buckets.len());
            buckets.push(Bucket {
                id: item.id,
                name: item.name,
                item_count: 1,
                size: item.size,
                top_items: vec![item.path],
                sort_name: item.sort_name.to_lowercase(),
            });
        }
    }
    buckets.sort_by(|a, b| a.sort_name.cmp(&b.sort_name).then_with(|| a.id.cmp(&b.id)));
    Ok(buckets)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/media_buckets.rs"]
mod tests;
