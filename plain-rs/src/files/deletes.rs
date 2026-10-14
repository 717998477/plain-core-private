use crate::db::Db;
use crate::enums::DataType;
use crate::error::DomainError;
use crate::error::DomainResult;
use crate::media::audio::queue as audio_queue;
use rusqlite::params;
#[derive(Clone)]
pub struct Item {
    pub media_type: i32,
    pub id: String,
    pub path: String,
}
pub fn cleanup(db: &Db, items: &[Item], paths: &[String], roots: &[String]) -> DomainResult<()> {
    if items.iter().any(|item| {
        !matches!(item.media_type, 1 | 2 | 3 | 24)
            || item.id.is_empty()
            || !paths.contains(&item.path)
    }) {
        return Err(DomainError::Other("invalid deleted media receipt".into()));
    }
    db.audio_transaction(|tx, state| {
        let encoded=serde_json::to_string(paths).map_err(|e|DomainError::Other(e.to_string()))?;
        let mut media = items.to_vec();
        {
            let mut statement=tx.prepare("SELECT data_type,id,path FROM media_items WHERE path IN (SELECT value FROM json_each(?1))")?;
            media.extend(statement.query_map([&encoded],|row|Ok(Item { media_type:row.get(0)?,id:row.get(1)?,path:row.get(2)? }))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        for root in roots {
            let prefix=directory_prefix(root);
            let mut statement=tx.prepare("SELECT data_type,id,path FROM media_items WHERE path=?1 OR substr(path,1,length(?2))=?2")?;
            media.extend(statement.query_map(params![root,prefix],|row|Ok(Item { media_type:row.get(0)?,id:row.get(1)?,path:row.get(2)? }))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        let mut seen=std::collections::HashSet::new();
        for item in &media {
            if !seen.insert((item.media_type,&item.id)) { continue; }
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND target_key=?2",params![item.media_type,item.id])?;
            if item.media_type==1 {
                for table in ["audio_queue_items","audio_playlist_items"] {
                    tx.execute(&format!("DELETE FROM {table} WHERE media_id=?1"),[&item.id])?;
                }
            }
            match item.media_type {
                1|2=>{
                    if matches!(item.media_type,1|2) { tx.execute("DELETE FROM media_play_history WHERE media_id=?1",[&item.id])?; }
                },
                3=>{tx.execute("DELETE FROM image_embeddings WHERE id=?1",[&item.id])?;}, _=>{},
            }
        }
        audio_queue::remove_paths_conn(&tx,paths, state)?;
        tx.execute("DELETE FROM media_items WHERE path IN (SELECT value FROM json_each(?1))",[&encoded])?;
        tx.execute("DELETE FROM tag_relations WHERE type=?1 AND target_key IN (SELECT value FROM json_each(?2))",params![DataType::File.kind(),encoded])?;
        tx.execute("DELETE FROM image_embeddings WHERE path IN (SELECT value FROM json_each(?1))",[&encoded])?;
        for root in roots {
            let prefix=directory_prefix(root);
            audio_queue::remove_root_paths_conn(&tx,root, state)?;
            tx.execute("DELETE FROM media_items WHERE path=?1 OR substr(path,1,length(?2))=?2",params![root,prefix])?;
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND (target_key=?2 OR substr(target_key,1,length(?3))=?3)",params![DataType::File.kind(),root,prefix])?;
            tx.execute("DELETE FROM image_embeddings WHERE path=?1 OR substr(path,1,length(?2))=?2",params![root,prefix])?;
        }
        Ok(())
    })
}
fn directory_prefix(root: &str) -> String {
    let separator = if cfg!(windows) && !root.contains('/') {
        '\\'
    } else {
        '/'
    };
    format!("{}{separator}", root.trim_end_matches(separator))
}

#[cfg(test)]
#[path = "../../tests/unit/files/deletes.rs"]
mod tests;
