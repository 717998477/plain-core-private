use super::*;
use mpeg2ts_reader::pes::ElementaryStreamConsumer;
use std::io::Cursor;

#[test]
fn real_h264_and_mpeg2_transport_streams_use_program_tables_and_codec_headers() {
    for (bytes, width, height) in [
        (
            include_bytes!("../../../testdata/video-h264.ts").as_slice(),
            52,
            38,
        ),
        (
            include_bytes!("../../../testdata/video-mpeg2.ts").as_slice(),
            64,
            48,
        ),
    ] {
        let info = crate::media::video_info::read(&mut Cursor::new(bytes)).unwrap();
        assert_eq!((info.width, info.height), (width, height));
        assert_eq!(info.frame_rate, 25.0);
        assert_eq!(info.duration_ms, 0);
        assert_eq!(
            crate::media::probe::probe_duration_reader(&mut Cursor::new(bytes)),
            None
        );
    }
}
#[test]
fn raw_m2ts_and_fec_packet_layouts_share_the_same_program_and_video_selection() {
    let bytes = include_bytes!("../../../testdata/video-h264.ts");
    for (prefix, suffix) in [(0, 0), (4, 0), (0, 16)] {
        let mut wrapped = Vec::new();
        for packet in bytes.chunks_exact(188) {
            wrapped.extend(vec![0; prefix]);
            wrapped.extend(packet);
            wrapped.extend(vec![0; suffix]);
        }
        let info = video(&mut Cursor::new(wrapped)).unwrap();
        assert_eq!((info.width, info.height), (52, 38));
    }
}
struct Counted {
    inner: Cursor<Vec<u8>>,
    bytes: usize,
}
impl Read for Counted {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(output)?;
        self.bytes += count;
        Ok(count)
    }
}
impl Seek for Counted {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(from)
    }
}
#[test]
fn descriptor_bounds_and_early_completion_do_not_read_the_entire_media_payload() {
    let mut bytes = include_bytes!("../../../testdata/video-h264.ts").to_vec();
    bytes.extend(vec![255; 8 * 1024 * 1024]);
    let length = bytes.len();
    let mut wrapped = b"outside".to_vec();
    wrapped.extend(bytes);
    wrapped.extend(b"outside");
    let mut counted = Counted {
        inner: Cursor::new(wrapped),
        bytes: 0,
    };
    let mut range =
        crate::utils::range_reader::RangeReader::new(&mut counted, 7, length as u64).unwrap();
    assert_eq!(video(&mut range).unwrap().width, 52);
    drop(range);
    assert!(counted.bytes < 16 * 1024, "read {} bytes", counted.bytes);
}
#[test]
fn corrupt_program_association_crc_cannot_be_bypassed_by_scanning_video_bytes() {
    let mut bytes = include_bytes!("../../../testdata/video-h264.ts").to_vec();
    let mut corrupted = 0;
    for packet in bytes.chunks_exact_mut(188) {
        let pid = (u16::from(packet[1] & 31) << 8) | u16::from(packet[2]);
        if pid != 0 {
            continue;
        }
        let payload = if packet[3] & 0x20 != 0 {
            5 + usize::from(packet[4])
        } else {
            4
        };
        let start = payload + 1 + usize::from(packet[payload]);
        let length = (usize::from(packet[start + 1] & 15) << 8) | usize::from(packet[start + 2]);
        packet[start + 3 + length - 1] ^= 1;
        corrupted += 1;
    }
    assert!(corrupted > 0);
    assert!(video(&mut Cursor::new(bytes)).is_none());
}
#[test]
fn elementary_prefix_is_bounded_and_continuity_errors_discard_partial_headers() {
    let mut context = Context::default();
    let mut consumer = Consumer {
        codec: StreamType::H262,
        prefix: Vec::new(),
    };
    consumer.push(&mut context, &[0, 0]);
    consumer.continuity_error(&mut context);
    consumer.push(&mut context, &[1, 0xb3, 4, 0, 48, 0x23]);
    assert!(context.video.is_none());
    consumer.prefix.clear();
    consumer.push(&mut context, &[0, 0]);
    consumer.push(&mut context, &[1, 0xb3, 4, 0, 48, 0x23]);
    assert_eq!(context.video.take().unwrap().width, 64);
    consumer.prefix.clear();
    consumer.push(&mut context, &vec![255; MAX_ELEMENTARY_BYTES * 2]);
    assert_eq!(consumer.prefix.len(), MAX_ELEMENTARY_BYTES);
    consumer.push(&mut context, &[0, 0, 1, 0xb3]);
    assert_eq!(consumer.prefix.len(), MAX_ELEMENTARY_BYTES);
}
#[test]
fn missing_sync_and_truncated_headers_remain_unknown() {
    assert!(video(&mut Cursor::new(vec![0; 1024])).is_none());
    let bytes = include_bytes!("../../../testdata/video-h264.ts");
    for end in [0, 4, 187, 188, 375] {
        assert!(video(&mut Cursor::new(&bytes[..end])).is_none());
    }
}

#[test]
fn real_hevc_transport_stream_keeps_conformance_dimensions_without_guessing_timing() {
    let bytes = include_bytes!("../../../testdata/video-hevc.ts");
    let info = video(&mut Cursor::new(bytes)).unwrap();
    assert_eq!((info.width, info.height), (96, 80));
    assert_eq!(info.frame_rate, 0.0);
    assert_eq!(
        crate::media::probe::probe_duration_reader(&mut Cursor::new(bytes)),
        None
    );
}
