//! 通过 HTTP Range 只读取 MP4 头尾，从 moov/tkhd 识别视频分辨率。
//!
//! 先 HEAD 取总长，再 Range 读取文件头部定位 `moov`；
//! `moov` 在尾部时按 `mdat` 结束位置再读尾段，最后解析
//! `moov -> trak -> tkhd` 的宽高（16.16 定点数）。

use crate::http::HttpClient;

const HEAD_BYTES: u64 = 4 * 1024 * 1024;
const MAX_HEAD_BYTES: usize = 4 * 1024 * 1024;
const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoDimensions {
    pub width: u32,
    pub height: u32,
}

/// 探测远端 MP4 的分辨率；任何一步失败都静默返回 None。
pub fn probe_dimensions(client: &HttpClient, url: &str) -> Option<VideoDimensions> {
    let total = client.head_content_length(url)?;
    let head = client.fetch_byte_range(url, 0, HEAD_BYTES - 1, MAX_HEAD_BYTES)?;
    if let Some(moov) = find_box(&head, 0, head.len(), b"moov") {
        return dimensions_from_moov(&head, moov);
    }
    let mdat = find_box(&head, 0, head.len(), b"mdat")?;
    let moov_start = mdat.start as u64 + mdat.size;
    if moov_start >= total {
        return None;
    }
    let tail = client.fetch_byte_range(url, moov_start, total - 1, MAX_TAIL_BYTES)?;
    let moov = find_box(&tail, 0, tail.len(), b"moov")?;
    dimensions_from_moov(&tail, moov)
}

struct BoxRef {
    /// box 头部（size + type）起始偏移。
    start: usize,
    /// box 总大小（含头部）。
    size: u64,
}

impl BoxRef {
    /// 跳过头部后的内容区间 [start, end)。
    fn contents(&self, data: &[u8]) -> Option<(usize, usize)> {
        let header = if self.size == 1 { 16 } else { 8 };
        let start = self.start + header;
        let end = self.start.checked_add(self.size as usize)?;
        if end <= start || end > data.len() {
            return None;
        }
        Some((start, end))
    }
}

fn box_kind(data: &[u8], start: usize) -> [u8; 4] {
    [
        data[start + 4],
        data[start + 5],
        data[start + 6],
        data[start + 7],
    ]
}

fn read_box(data: &[u8], pos: usize) -> Option<BoxRef> {
    if pos + 8 > data.len() {
        return None;
    }
    let mut size = u32::from_be_bytes(data[pos..pos + 4].try_into().ok()?) as u64;
    if size == 1 {
        if pos + 16 > data.len() {
            return None;
        }
        size = u64::from_be_bytes(data[pos + 8..pos + 16].try_into().ok()?);
        if size < 16 {
            return None;
        }
    } else if size < 8 {
        return None;
    }
    Some(BoxRef { start: pos, size })
}

fn find_box(data: &[u8], start: usize, end: usize, kind: &[u8; 4]) -> Option<BoxRef> {
    let mut pos = start;
    while pos + 8 <= end {
        let found = read_box(data, pos)?;
        if &box_kind(data, pos) == kind {
            return Some(found);
        }
        pos += found.size as usize;
    }
    None
}

fn dimensions_from_moov(data: &[u8], moov: BoxRef) -> Option<VideoDimensions> {
    let (start, end) = moov.contents(data)?;
    for trak in child_boxes(data, start, end, b"trak") {
        if let Some(dimensions) = dimensions_from_trak(data, trak) {
            return Some(dimensions);
        }
    }
    None
}

fn dimensions_from_trak(data: &[u8], trak: BoxRef) -> Option<VideoDimensions> {
    let (start, end) = trak.contents(data)?;
    for tkhd in child_boxes(data, start, end, b"tkhd") {
        let (_, tkhd_end) = tkhd.contents(data)?;
        // tkhd 以 width + height（各 16.16 定点数）结尾。
        let bytes = &data[tkhd_end - 8..tkhd_end];
        let width = u32::from_be_bytes(bytes[0..4].try_into().ok()?) / 65536;
        let height = u32::from_be_bytes(bytes[4..8].try_into().ok()?) / 65536;
        if width > 0 && height > 0 {
            return Some(VideoDimensions { width, height });
        }
    }
    None
}

fn child_boxes(data: &[u8], start: usize, end: usize, kind: &[u8; 4]) -> Vec<BoxRef> {
    let mut children = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let Some(child) = read_box(data, pos) else {
            break;
        };
        let child_size = child.size;
        if &box_kind(data, pos) == kind {
            children.push(child);
        }
        pos += child_size as usize;
    }
    children
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_bytes(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = (8 + payload.len()) as u32;
        let mut out = size.to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    fn tkhd_bytes(width: u32, height: u32) -> Vec<u8> {
        box_bytes(b"tkhd", &{
            let mut payload = vec![0u8; 20];
            payload.extend_from_slice(&(width * 65536).to_be_bytes());
            payload.extend_from_slice(&(height * 65536).to_be_bytes());
            payload
        })
    }

    fn sample_moov(video_dims: (u32, u32), audio_dims: (u32, u32)) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&box_bytes(b"mvhd", &[0u8; 100]));
        payload.extend_from_slice(&box_bytes(b"trak", &{
            let mut trak = tkhd_bytes(video_dims.0, video_dims.1);
            trak.extend_from_slice(&box_bytes(b"mdia", &[0u8; 16]));
            trak
        }));
        payload.extend_from_slice(&box_bytes(b"trak", &{
            let mut trak = tkhd_bytes(audio_dims.0, audio_dims.1);
            trak.extend_from_slice(&box_bytes(b"mdia", &[0u8; 16]));
            trak
        }));
        box_bytes(b"moov", &payload)
    }

    #[test]
    fn dimensions_from_moov_reads_video_track_tkhd() {
        let moov = sample_moov((3840, 2160), (0, 0));
        let moov_ref = BoxRef {
            start: 0,
            size: moov.len() as u64,
        };
        let dimensions = dimensions_from_moov(&moov, moov_ref);
        assert_eq!(
            dimensions,
            Some(VideoDimensions {
                width: 3840,
                height: 2160,
            })
        );
    }

    #[test]
    fn audio_only_trak_yields_none() {
        let moov = sample_moov((0, 0), (0, 0));
        let moov_ref = BoxRef {
            start: 0,
            size: moov.len() as u64,
        };
        assert_eq!(dimensions_from_moov(&moov, moov_ref), None);
    }

    #[test]
    fn find_box_scans_past_unknown_boxes() {
        let mut data = box_bytes(b"free", &[0u8; 8]);
        let moov = box_bytes(b"moov", &[0u8; 16]);
        let offset = data.len();
        data.extend_from_slice(&moov);
        let found = find_box(&data, 0, data.len(), b"moov").unwrap();
        assert_eq!(found.start, offset);
    }
}
