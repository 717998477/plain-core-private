use super::{fetch_text, parse_feed};
use plain_rs::db::Db;
use plain_rs::error::DomainResult;
use plain_rs::feeds::{error, require_url};

pub async fn title(db: &Db, url: &str) -> DomainResult<String> {
    require_url(url)?;
    if db.feed_get_by_url(url)?.is_some() {
        return Err(error("feed_already_exists"));
    }
    Ok(parse_feed(&fetch_text(url, 4 * 1024 * 1024).await?)?.title)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/feeds/preview.rs"]
mod tests;
