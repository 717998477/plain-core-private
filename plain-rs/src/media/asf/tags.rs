use super::{u16_at, u32_at};
use crate::media::video_info::VideoInfo;
use std::io::{Read, Seek, SeekFrom};

pub(super) fn utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units = bytes
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units)
        .ok()
        .map(|text| text.trim_end_matches('\0').to_owned())
}
fn read(reader: &mut (impl Read + Seek), offset: u64, length: u64, end: u64) -> Option<Vec<u8>> {
    if length > 64 * 1024 || offset.checked_add(length)? > end {
        return None;
    }
    reader.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; length as usize];
    reader.read_exact(&mut bytes).ok()?;
    Some(bytes)
}
pub(super) fn content(
    reader: &mut (impl Read + Seek),
    start: u64,
    end: u64,
    info: &mut VideoInfo,
) -> Option<()> {
    let sizes = read(reader, start, 10, end)?;
    let mut offset = start + 10;
    for index in 0..5 {
        let size = u64::from(u16_at(&sizes, index * 2)?);
        if offset.checked_add(size)? > end {
            return None;
        }
        if index < 2
            && let Some(text) = utf16(&read(reader, offset, size, end)?)
        {
            if index == 0 {
                info.title = text;
            } else {
                info.artist = text;
            }
        }
        offset += size;
    }
    Some(())
}
pub(super) fn extended(
    reader: &mut (impl Read + Seek),
    start: u64,
    end: u64,
    info: &mut VideoInfo,
) -> Option<()> {
    let count = u16_at(&read(reader, start, 2, end)?, 0)?;
    let mut offset = start + 2;
    for _ in 0..count {
        let name_size = u64::from(u16_at(&read(reader, offset, 2, end)?, 0)?);
        offset += 2;
        let name = utf16(&read(reader, offset, name_size, end)?);
        offset += name_size;
        let properties = read(reader, offset, 4, end)?;
        let kind = u16_at(&properties, 0)?;
        let size = u64::from(u16_at(&properties, 2)?);
        offset += 4;
        if offset.checked_add(size)? > end {
            return None;
        }
        if let Some(name) = name
            && selected(&name)
        {
            let bytes = read(reader, offset, size, end)?;
            let value = if kind == 0 {
                utf16(&bytes)
            } else if kind == 3 && name == "WM/VideoFrameRate" {
                Some(u32_at(&bytes, 0)?.to_string())
            } else {
                None
            };
            if let Some(value) = value {
                apply(info, &name, value);
            }
        }
        offset += size;
    }
    Some(())
}
fn selected(name: &str) -> bool {
    matches!(
        name,
        "Title"
            | "Author"
            | "WM/AlbumArtist"
            | "WM/AlbumTitle"
            | "WM/Genre"
            | "WM/Writer"
            | "WM/Composer"
            | "WM/VideoFrameRate"
            | "WM/MediaOriginalBroadcastDateTime"
    )
}
fn apply(info: &mut VideoInfo, name: &str, value: String) {
    match name {
        "Title" => info.title = value,
        "Author" => info.artist = value,
        "WM/AlbumArtist" if info.artist.is_empty() => info.artist = value,
        "WM/AlbumTitle" => info.album = value,
        "WM/Genre" => info.genre = value,
        "WM/Writer" => info.writer = value,
        "WM/Composer" => info.composer = value,
        "WM/MediaOriginalBroadcastDateTime" => info.raw_taken_at = Some(value),
        "WM/VideoFrameRate" => {
            if let Ok(rate) = value.parse::<f64>()
                && rate.is_finite()
                && rate > 0.0
            {
                info.frame_rate = rate;
            }
        }
        _ => {}
    }
}
