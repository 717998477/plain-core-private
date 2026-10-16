mod id3;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, TagType};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
};

#[derive(Default)]
pub struct ProbedMeta {
    pub duration_secs: u32,
    pub duration_ms: i64,
    pub duration_available: bool,
    pub artist: String,
    pub title: String,
    /// False preserves existing tags when the file cannot be parsed.
    pub parsed: bool,
}

pub fn probe_media(path: &str, kind: &str) -> ProbedMeta {
    if !matches!(kind, "audio" | "video") {
        return ProbedMeta::default();
    }
    if kind != "audio" {
        let duration = probe_duration_milliseconds(path);
        return ProbedMeta {
            duration_secs: seconds(duration.unwrap_or_default()),
            duration_ms: duration.unwrap_or_default(),
            duration_available: duration.is_some(),
            artist: String::new(),
            title: String::new(),
            parsed: duration.is_some(),
        };
    }
    let Ok(mut reader) = File::open(path) else {
        return ProbedMeta::default();
    };
    probe_audio_reader(&mut reader)
}

pub fn probe_audio_reader(reader: &mut (impl Read + Seek)) -> ProbedMeta {
    if reader.seek(SeekFrom::Start(0)).is_err() {
        return ProbedMeta::default();
    }
    let mut signature = [0; 16];
    if reader.read_exact(&mut signature).is_ok() && signature == super::asf::HEADER {
        return super::asf::metadata(reader)
            .map(|(facts, duration)| ProbedMeta {
                duration_secs: seconds(duration.unwrap_or_default()),
                duration_ms: duration.unwrap_or_default(),
                duration_available: duration.is_some(),
                artist: facts.artist,
                title: facts.title,
                parsed: true,
            })
            .unwrap_or_default();
    }
    if reader.seek(SeekFrom::Start(0)).is_err() {
        return ProbedMeta::default();
    }
    let Ok(projected) = id3::reader(&mut *reader, true) else {
        return ProbedMeta::default();
    };
    let result = Probe::new(BufReader::new(projected))
        .options(ParseOptions::new().read_cover_art(false))
        .guess_file_type()
        .ok()
        .and_then(|probe| probe.read().ok());
    let (mut duration, artist, title, parsed) = match result {
        Some(file) => {
            let (artist, title) = tag_strings(&file);
            (
                i64::try_from(file.properties().duration().as_millis()).ok(),
                artist,
                title,
                true,
            )
        }
        None => (None, String::new(), String::new(), false),
    };
    if duration.is_none_or(|duration| duration == 0) {
        let raw = super::raw_audio::milliseconds(reader);
        let fallback = raw.or_else(|| {
            reader
                .seek(SeekFrom::End(0))
                .ok()
                .and_then(|length| super::isobmff::mp4_file_milliseconds(reader, 0, length))
        });
        duration = fallback.or(duration);
    }
    ProbedMeta {
        duration_secs: seconds(duration.unwrap_or_default()),
        duration_ms: duration.unwrap_or_default(),
        duration_available: duration.is_some(),
        artist,
        title,
        parsed,
    }
}

fn seconds(milliseconds: i64) -> u32 {
    u32::try_from(milliseconds.max(0) / 1000).unwrap_or(u32::MAX)
}

pub fn probe_duration_secs(path: &str) -> Option<u32> {
    u32::try_from(probe_duration_milliseconds(path)? / 1000).ok()
}

pub fn probe_duration_milliseconds(path: &str) -> Option<i64> {
    let mut reader = File::open(path).ok()?;
    probe_duration_reader(&mut reader)
}

/// None is unreadable; Some(0) is parsed without a measurable duration.
pub fn probe_duration_reader(reader: &mut (impl Read + Seek)) -> Option<i64> {
    if let Some(duration) = super::raw_audio::milliseconds(reader) {
        return Some(duration);
    }
    if let Some(duration) = super::flv::milliseconds(reader) {
        return Some(duration);
    }
    if let Some(duration) = super::avi::milliseconds(reader) {
        return Some(duration);
    }
    if let Some(duration) = super::matroska::milliseconds(reader) {
        return Some(duration);
    }
    let length = reader.seek(SeekFrom::End(0)).ok()?;
    if let Some(duration) = super::isobmff::mp4_file_milliseconds(reader, 0, length) {
        return Some(duration);
    }
    reader.seek(SeekFrom::Start(0)).ok()?;
    let projected = id3::reader(reader, false).ok()?;
    let file = Probe::new(BufReader::new(projected))
        .options(ParseOptions::new().read_tags(false))
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;
    i64::try_from(file.properties().duration().as_millis()).ok()
}

fn tag_strings(tagged_file: &lofty::file::TaggedFile) -> (String, String) {
    let mut artist = String::new();
    let mut title = String::new();
    for tag_type in [
        TagType::Id3v2,
        TagType::Ape,
        TagType::Id3v1,
        TagType::VorbisComments,
        TagType::Mp4Ilst,
    ] {
        if let Some(tag) = tagged_file.tag(tag_type) {
            if artist.is_empty()
                && let Some(a) = tag.artist()
                && !a.trim().is_empty()
            {
                artist = a.trim().to_string();
            }
            if title.is_empty()
                && let Some(t) = tag.title()
                && !t.trim().is_empty()
            {
                title = t.trim().to_string();
            }
        }
    }
    if artist.is_empty() || title.is_empty() {
        if let Some(tag) = tagged_file
            .primary_tag()
            .or_else(|| tagged_file.first_tag())
        {
            if artist.is_empty()
                && let Some(a) = tag.artist()
                && !a.trim().is_empty()
            {
                artist = a.trim().to_string();
            }
            if title.is_empty()
                && let Some(t) = tag.title()
                && !t.trim().is_empty()
            {
                title = t.trim().to_string();
            }
        }
    }
    (artist, title)
}

#[cfg(test)]
#[path = "../../tests/unit/media/probe.rs"]
mod tests;
