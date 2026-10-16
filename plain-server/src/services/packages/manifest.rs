use anyhow::{Result, anyhow, ensure};
use std::io::{Read, Seek};

pub(super) const MANIFEST_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(super) struct Metadata {
    pub package_name: String,
    pub split: Option<String>,
}

pub(super) fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(
                offset
                    ..offset
                        .checked_add(2)
                        .ok_or_else(|| anyhow!("APK offset overflow"))?,
            )
            .ok_or_else(|| anyhow!("Truncated APK manifest"))?
            .try_into()?,
    ))
}
pub(super) fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(
                offset
                    ..offset
                        .checked_add(4)
                        .ok_or_else(|| anyhow!("APK offset overflow"))?,
            )
            .ok_or_else(|| anyhow!("Truncated APK manifest"))?
            .try_into()?,
    ))
}
fn length(bytes: &[u8], cursor: &mut usize, utf8: bool) -> Result<usize> {
    if utf8 {
        let first = *bytes
            .get(*cursor)
            .ok_or_else(|| anyhow!("Truncated manifest string"))?;
        *cursor += 1;
        if first & 0x80 == 0 {
            return Ok(first as usize);
        }
        let second = *bytes
            .get(*cursor)
            .ok_or_else(|| anyhow!("Truncated manifest string"))?;
        *cursor += 1;
        Ok(((first as usize & 0x7f) << 8) | second as usize)
    } else {
        let first = u16_at(bytes, *cursor)?;
        *cursor += 2;
        if first & 0x8000 == 0 {
            return Ok(first as usize);
        }
        let second = u16_at(bytes, *cursor)?;
        *cursor += 2;
        Ok(((first as usize & 0x7fff) << 16) | second as usize)
    }
}
pub(super) struct Pool<'a> {
    bytes: &'a [u8],
    header: usize,
    count: usize,
    start: usize,
    utf8: bool,
    remaining: std::cell::Cell<usize>,
}
impl<'a> Pool<'a> {
    pub(super) fn new(chunk: &'a [u8], header: usize) -> Result<Self> {
        ensure!(header >= 28, "Invalid manifest string pool");
        let count = u32_at(chunk, 8)? as usize;
        let styles = u32_at(chunk, 12)? as usize;
        let start = u32_at(chunk, 20)? as usize;
        let style_start = u32_at(chunk, 24)? as usize;
        let table_end = header
            .checked_add(
                count
                    .checked_add(styles)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or_else(|| anyhow!("Manifest string count overflow"))?,
            )
            .ok_or_else(|| anyhow!("Manifest string table overflow"))?;
        ensure!(
            table_end <= start && start <= chunk.len(),
            "Invalid manifest string offsets"
        );
        let end = if style_start == 0 {
            chunk.len()
        } else {
            style_start
        };
        ensure!(
            start <= end && end <= chunk.len(),
            "Invalid manifest style offset"
        );
        Ok(Self {
            bytes: &chunk[..end],
            header,
            count,
            start,
            utf8: u32_at(chunk, 16)? & 0x100 != 0,
            remaining: std::cell::Cell::new(MANIFEST_LIMIT as usize),
        })
    }
    #[cfg(test)]
    pub(super) fn remaining_bytes(&self) -> usize {
        self.remaining.get()
    }
    pub(super) fn get(&self, index: u32) -> Result<String> {
        let index = index as usize;
        ensure!(index < self.count, "Invalid manifest string index");
        let offset = u32_at(self.bytes, self.header + index * 4)? as usize;
        let mut cursor = self
            .start
            .checked_add(offset)
            .ok_or_else(|| anyhow!("Manifest string offset overflow"))?;
        let utf16_len = length(self.bytes, &mut cursor, self.utf8)?;
        let size = if self.utf8 {
            length(self.bytes, &mut cursor, true)?
        } else {
            utf16_len
                .checked_mul(2)
                .ok_or_else(|| anyhow!("Manifest string length overflow"))?
        };
        let finish = cursor
            .checked_add(size)
            .ok_or_else(|| anyhow!("Manifest string length overflow"))?;
        let text = self
            .bytes
            .get(cursor..finish)
            .ok_or_else(|| anyhow!("Truncated manifest string"))?;
        self.remaining.set(
            self.remaining
                .get()
                .checked_sub(size)
                .ok_or_else(|| anyhow!("Manifest string budget exceeded"))?,
        );
        if self.utf8 {
            let text = std::str::from_utf8(text)?;
            ensure!(
                self.bytes.get(finish) == Some(&0) && text.encode_utf16().count() == utf16_len,
                "Invalid UTF-8 manifest string"
            );
            Ok(text.to_owned())
        } else {
            ensure!(
                u16_at(self.bytes, finish)? == 0,
                "Invalid UTF-16 manifest terminator"
            );
            Ok(String::from_utf16(
                &text
                    .chunks_exact(2)
                    .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                    .collect::<Vec<_>>(),
            )?)
        }
    }
}
fn root(bytes: &[u8]) -> Result<Metadata> {
    ensure!(u16_at(bytes, 0)? == 3, "Binary Android manifest required");
    let mut offset = u16_at(bytes, 2)? as usize;
    let size = u32_at(bytes, 4)? as usize;
    ensure!(
        offset >= 8 && offset <= size && size == bytes.len(),
        "Invalid manifest extent"
    );
    let mut pool = None;
    while offset < size {
        let kind = u16_at(bytes, offset)?;
        let header = u16_at(bytes, offset + 2)? as usize;
        let length = u32_at(bytes, offset + 4)? as usize;
        ensure!(
            header >= 8 && length >= header && length <= size - offset,
            "Invalid manifest chunk"
        );
        let chunk = &bytes[offset..offset + length];
        match kind {
            1 => {
                ensure!(pool.is_none(), "Duplicate manifest string pool");
                pool = Some(Pool::new(chunk, header)?);
            }
            0x102 => {
                ensure!(
                    header >= 16 && header + 20 <= length,
                    "Invalid manifest element"
                );
                let pool = pool
                    .as_ref()
                    .ok_or_else(|| anyhow!("Missing manifest string pool"))?;
                ensure!(
                    pool.get(u32_at(chunk, header + 4)?)? == "manifest",
                    "Manifest root required"
                );
                let start = header + u16_at(chunk, header + 8)? as usize;
                let stride = u16_at(chunk, header + 10)? as usize;
                let count = u16_at(chunk, header + 12)? as usize;
                ensure!(
                    start >= header + 20
                        && stride >= 20
                        && start <= length
                        && count <= (length - start) / stride,
                    "Invalid manifest attributes"
                );
                let mut package_name = None;
                let mut split = None;
                for index in 0..count {
                    let position = start + index * stride;
                    let namespace = u32_at(chunk, position)?;
                    if namespace != u32::MAX {
                        continue;
                    }
                    let name = pool.get(u32_at(chunk, position + 4)?)?;
                    if !matches!(name.as_str(), "package" | "split") {
                        continue;
                    }
                    let raw = u32_at(chunk, position + 8)?;
                    let typed = *chunk
                        .get(position + 15)
                        .ok_or_else(|| anyhow!("Truncated manifest value"))?;
                    ensure!(
                        u16_at(chunk, position + 12)? >= 8,
                        "Invalid manifest typed value"
                    );
                    let value = if raw != u32::MAX {
                        pool.get(raw)?
                    } else {
                        ensure!(typed == 3, "Manifest identity must be a string");
                        pool.get(u32_at(chunk, position + 16)?)?
                    };
                    let destination = if name == "package" {
                        &mut package_name
                    } else {
                        &mut split
                    };
                    ensure!(destination.is_none(), "Duplicate manifest identity");
                    *destination = Some(value);
                }
                let package_name = package_name
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow!("Missing APK package name"))?;
                ensure!(
                    package_name
                        .chars()
                        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '_' | '.')),
                    "Invalid APK package name"
                );
                return Ok(Metadata {
                    package_name,
                    split: split.filter(|value| !value.is_empty()),
                });
            }
            _ => {}
        }
        offset += length;
    }
    Err(anyhow!("Manifest root not found"))
}
pub(super) fn read(reader: impl Read + Seek) -> Result<Metadata> {
    root(&bytes(reader)?)
}
fn entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    ensure!(entry.size() <= limit, "APK entry {name} is too large");
    let expected = entry.size();
    let mut bytes = Vec::new();
    (&mut entry).take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit && bytes.len() as u64 == expected,
        "Invalid APK entry length"
    );
    Ok(Some(bytes))
}
pub(super) fn bytes(reader: impl Read + Seek) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(reader)?;
    entry(&mut archive, "AndroidManifest.xml", MANIFEST_LIMIT)?
        .ok_or_else(|| anyhow!("APK manifest not found"))
}
pub(super) fn xml(reader: impl Read + Seek) -> Result<String> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let bytes = entry(&mut archive, "AndroidManifest.xml", MANIFEST_LIMIT)?
        .ok_or_else(|| anyhow!("APK manifest not found"))?;
    let resources = if super::xml::needs_resources(&bytes)? {
        entry(&mut archive, "resources.arsc", super::resources::LIMIT)?
    } else {
        None
    };
    let resources = resources
        .as_deref()
        .map(super::resources::Resources::parse)
        .transpose()?;
    super::xml::render_with(&bytes, resources.as_ref())
}

#[cfg(test)]
#[path = "../../../tests/unit/services/packages/manifest.rs"]
pub(crate) mod tests;
