use super::video_info::{StreamInfo, VideoInfo};
use mpeg2ts_reader::{
    StreamType,
    demultiplex::{self, DemuxContext, FilterChangeset, FilterRequest, PacketFilter},
    packet::{Packet, Pid},
    pes, psi,
};
use std::io::{BufReader, Read, Seek, SeekFrom};

const MAX_PACKETS: u64 = 4096;
const MAX_ELEMENTARY_BYTES: usize = 64 * 1024;

enum Filter {
    Pat(demultiplex::PatPacketFilter<Context>),
    Pmt(demultiplex::PmtPacketFilter<Context>),
    Pes(pes::PesPacketFilter<Context, Consumer>),
    Null(demultiplex::NullPacketFilter<Context>),
}
impl PacketFilter for Filter {
    type Ctx = Context;
    fn consume(&mut self, context: &mut Context, packet: &Packet<'_>) {
        match self {
            Self::Pat(filter) => filter.consume(context, packet),
            Self::Pmt(filter) => filter.consume(context, packet),
            Self::Pes(filter) => filter.consume(context, packet),
            Self::Null(filter) => filter.consume(context, packet),
        }
    }
}
#[derive(Default)]
struct Context {
    changes: FilterChangeset<Filter>,
    program: Option<Pid>,
    video_pid: Option<Pid>,
    video: Option<StreamInfo>,
}
impl DemuxContext for Context {
    type F = Filter;
    fn filter_changeset(&mut self) -> &mut FilterChangeset<Filter> {
        &mut self.changes
    }
    fn construct(&mut self, request: FilterRequest<'_, '_>) -> Filter {
        match request {
            FilterRequest::ByPid(psi::pat::PAT_PID) => Filter::Pat(Default::default()),
            FilterRequest::Pmt {
                pid,
                program_number,
            } if self.program.is_none() || self.program == Some(pid) => {
                self.program = Some(pid);
                Filter::Pmt(demultiplex::PmtPacketFilter::new(pid, program_number))
            }
            FilterRequest::ByStream {
                program_pid,
                stream_type,
                stream_info,
                ..
            } if self.program == Some(program_pid)
                && (self.video_pid.is_none()
                    || self.video_pid == Some(stream_info.elementary_pid()))
                && matches!(
                    stream_type,
                    StreamType::H264
                        | StreamType::H262
                        | StreamType::ISO_11172_VIDEO
                        | StreamType::H265
                ) =>
            {
                self.video_pid = Some(stream_info.elementary_pid());
                Filter::Pes(pes::PesPacketFilter::new(Consumer {
                    codec: stream_type,
                    prefix: Vec::new(),
                }))
            }
            _ => Filter::Null(Default::default()),
        }
    }
}
struct Consumer {
    codec: StreamType,
    prefix: Vec<u8>,
}
impl Consumer {
    fn push(&mut self, context: &mut Context, bytes: &[u8]) {
        if context.video.is_some() {
            return;
        }
        let available = MAX_ELEMENTARY_BYTES.saturating_sub(self.prefix.len());
        if available == 0 || bytes.is_empty() {
            return;
        }
        self.prefix
            .extend_from_slice(&bytes[..bytes.len().min(available)]);
        context.video = elementary(self.codec, &self.prefix);
    }
}
impl pes::ElementaryStreamConsumer<Context> for Consumer {
    fn start_stream(&mut self, _: &mut Context) {}
    fn begin_packet(&mut self, context: &mut Context, header: pes::PesHeader<'_>) {
        self.prefix.clear();
        match header.contents() {
            pes::PesContents::Parsed(Some(contents)) => self.push(context, contents.payload()),
            pes::PesContents::Payload(bytes) => self.push(context, bytes),
            pes::PesContents::Parsed(None) => {}
        }
    }
    fn continue_packet(&mut self, context: &mut Context, data: &[u8]) {
        self.push(context, data);
    }
    fn end_packet(&mut self, _: &mut Context) {}
    fn continuity_error(&mut self, _: &mut Context) {
        self.prefix.clear();
    }
}
fn elementary(codec: StreamType, bytes: &[u8]) -> Option<StreamInfo> {
    let mut offset = 0;
    while offset + 4 <= bytes.len() {
        if bytes.get(offset..offset + 3)? != [0, 0, 1] {
            offset += 1;
            continue;
        }
        let start = offset + 3;
        offset = start;
        if (codec == StreamType::H264 && bytes[start] & 31 == 7)
            || (codec == StreamType::H265 && (bytes[start] >> 1) & 63 == 33)
        {
            let end = bytes[start + 1..]
                .windows(3)
                .position(|value| value == [0, 0, 1])
                .map(|position| start + 1 + position)
                .unwrap_or(bytes.len());
            let info = if codec == StreamType::H264 {
                super::h264::sps(&bytes[start..end])
            } else {
                super::hevc::sps(&bytes[start..end])
            };
            if info.is_some() {
                return info;
            }
        }
        if matches!(codec, StreamType::H262 | StreamType::ISO_11172_VIDEO) && bytes[start] == 0xb3 {
            let mut bits = super::bits::Bits::new(bytes.get(start + 1..)?);
            let width = bits.read(12)?;
            let height = bits.read(12)?;
            bits.read(4)?;
            let rate = match bits.read(4)? {
                1 => Some(24000.0 / 1001.0),
                2 => Some(24.0),
                3 => Some(25.0),
                4 => Some(30000.0 / 1001.0),
                5 => Some(30.0),
                6 => Some(50.0),
                7 => Some(60000.0 / 1001.0),
                8 => Some(60.0),
                _ => None,
            };
            if width > 0 && height > 0 {
                return Some(StreamInfo {
                    width,
                    height,
                    frame_rate: rate,
                });
            }
        }
    }
    None
}
fn layout(bytes: &[u8]) -> Option<(usize, usize)> {
    for stride in [188, 192, 204] {
        for offset in 0..stride {
            if (0..3).all(|index| bytes.get(offset + index * stride) == Some(&0x47)) {
                return Some((offset, stride));
            }
        }
    }
    None
}
pub(super) fn video(reader: &mut (impl Read + Seek)) -> Option<VideoInfo> {
    let length = reader.seek(SeekFrom::End(0)).ok()?;
    reader.seek(SeekFrom::Start(0)).ok()?;
    let mut probe = vec![0; length.min(1024) as usize];
    reader.read_exact(&mut probe).ok()?;
    let (offset, stride) = layout(&probe)?;
    reader.seek(SeekFrom::Start(offset as u64)).ok()?;
    let packets = length.checked_sub(offset as u64)?.checked_sub(188)? / stride as u64 + 1;
    let mut reader = BufReader::with_capacity(stride * 32, reader);
    let mut context = Context::default();
    let mut demux = demultiplex::Demultiplex::new(&mut context);
    let mut packet = [0; 204];
    for index in 0..packets.min(MAX_PACKETS) {
        reader.read_exact(&mut packet[..188]).ok()?;
        if packet[0] != 0x47 {
            return None;
        }
        demux.push(&mut context, &packet[..188]);
        if let Some(video) = context.video.take() {
            return Some(VideoInfo {
                width: video.width,
                height: video.height,
                frame_rate: video.frame_rate.unwrap_or_default(),
                ..Default::default()
            });
        }
        if index + 1 < packets {
            reader.read_exact(&mut packet[..stride - 188]).ok()?;
        }
    }
    None
}

#[cfg(test)]
#[path = "../../tests/unit/media/transport_stream.rs"]
mod tests;
