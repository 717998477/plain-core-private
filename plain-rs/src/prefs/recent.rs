use super::Prefs;
use anyhow::Result;

const KEY: &str = "recent_files";
const LIMIT: usize = 500;

pub fn add_recent_file(prefs: &Prefs, path: &str) -> Result<()> {
    Ok(prefs.update_system(|values| {
        let mut paths: Vec<String> = values
            .get(KEY)
            .cloned()
            .map(|value| serde_json::from_value(value).map_err(super::PrefsError::Decode))
            .transpose()?
            .unwrap_or_default();
        paths.retain(|value| value != path);
        paths.insert(0, path.to_owned());
        paths.truncate(LIMIT);
        values.insert(
            KEY.into(),
            serde_json::to_value(paths).map_err(super::PrefsError::Encode)?,
        );
        Ok(())
    })?)
}

pub fn get_recent_files(prefs: &Prefs, limit: usize) -> Result<Vec<String>> {
    let mut paths: Vec<String> = prefs.get(KEY)?.unwrap_or_default();
    if limit > 0 {
        paths.truncate(limit);
    }
    Ok(paths)
}

#[cfg(test)]
#[path = "../../tests/unit/prefs/recent.rs"]
mod tests;
