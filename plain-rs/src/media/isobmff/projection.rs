use super::header;
use std::io::{Read, Seek, SeekFrom};

const LIMIT: usize = 4 * 1024 * 1024;

pub(super) fn movie(reader: &mut (impl Read + Seek), start: u64, end: u64) -> Option<Vec<u8>> {
    children(reader, start, end, b"moov", 0)
}
fn bytes(reader: &mut (impl Read + Seek), start: u64, length: u64) -> Option<Vec<u8>> {
    let length = usize::try_from(length).ok()?;
    if length > LIMIT {
        return None;
    }
    reader.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).ok()?;
    Some(bytes)
}
fn atom(output: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) -> Option<()> {
    let size = data.len().checked_add(8)?;
    if output.len().checked_add(size)? > LIMIT {
        return None;
    }
    output.extend(u32::try_from(size).ok()?.to_be_bytes());
    output.extend(kind);
    output.extend(data);
    Some(())
}
fn children(
    reader: &mut (impl Read + Seek),
    mut start: u64,
    end: u64,
    parent: &[u8; 4],
    depth: u8,
) -> Option<Vec<u8>> {
    if depth > 10 {
        return None;
    }
    let mut output = Vec::new();
    while start < end {
        let (kind, body, next) = header(reader, start, end)?;
        start = next;
        let length = next - body;
        let data = if parent == b"stsd" {
            Some(bytes(reader, body, length.min(28))?)
        } else if parent == b"ilst" {
            let index = u32::from_be_bytes(kind);
            if (1..=8192).contains(&index)
                || matches!(
                    &kind,
                    b"\xa9nam"
                        | b"\xa9ART"
                        | b"\xa9alb"
                        | b"\xa9gen"
                        | b"gnre"
                        | b"\xa9wrt"
                        | b"\xa9day"
                )
            {
                Some(children(reader, body, next, &kind, depth + 1)?)
            } else {
                None
            }
        } else {
            match &kind {
                b"trak" | b"mdia" | b"minf" | b"stbl" | b"udta" | b"ilst" => {
                    Some(children(reader, body, next, &kind, depth + 1)?)
                }
                b"meta" => {
                    if length < 4 {
                        return None;
                    }
                    let prefix = bytes(reader, body, 4)?;
                    let mut data = Vec::new();
                    let offset = if prefix == [0; 4] {
                        data.extend([0; 4]);
                        4
                    } else {
                        0
                    };
                    data.extend(children(reader, body + offset, next, &kind, depth + 1)?);
                    Some(data)
                }
                b"stsd" => {
                    if length < 8 {
                        return None;
                    }
                    let mut data = bytes(reader, body, 8)?;
                    data.extend(children(reader, body + 8, next, &kind, depth + 1)?);
                    Some(data)
                }
                b"tkhd" => Some(bytes(reader, body, length.min(96))?),
                b"mvhd" | b"mdhd" => Some(bytes(reader, body, length.min(32))?),
                b"hdlr" => Some(bytes(reader, body, length.min(12))?),
                b"stts" => Some(bytes(reader, body, length)?),
                b"titl" | b"perf" | b"albm" | b"gnre" | b"auth" | b"keys" | b"data"
                | b"\xa9nam" | b"\xa9ART" | b"\xa9alb" | b"\xa9gen" | b"\xa9wrt" | b"\xa9day"
                    if length <= 64 * 1024 =>
                {
                    Some(bytes(reader, body, length)?)
                }
                _ => None,
            }
        };
        if let Some(data) = data {
            atom(&mut output, kind, &data)?;
        }
    }
    Some(output)
}
