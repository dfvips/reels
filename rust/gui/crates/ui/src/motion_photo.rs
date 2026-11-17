//! 将图片与实况动轨合成为 Google 标准的 Motion Photo（JPEG 内嵌 MP4）。

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// JPEG APP1 中标识 XMP 数据包的前缀。
const XMP_IDENTIFIER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

/// 写入单文件 Motion Photo：JPEG + XMP 容器描述 + 末尾追加的视频。
/// 非 JPEG 图片会按高质量重新编码为 JPEG，视频数据原样追加。
pub fn write(image_path: &Path, video_path: &Path, output_path: &Path) -> Result<(), String> {
    let video = std::fs::read(video_path).map_err(|error| format!("读取实况视频失败: {error}"))?;
    if video.is_empty() {
        return Err("实况视频为空".to_string());
    }
    let image = std::fs::read(image_path).map_err(|error| format!("读取图片失败: {error}"))?;
    let jpeg = as_jpeg(image_path, &image)?;
    let stripped = strip_xmp_segments(&jpeg);
    let jpeg = trim_to_eoi(&stripped)?;

    let mut payload = XMP_IDENTIFIER.to_vec();
    payload.extend_from_slice(build_xmp(video.len()).as_bytes());
    if payload.len() + 2 > u16::MAX as usize {
        return Err("XMP 元数据过大".to_string());
    }

    let file = File::create(output_path).map_err(|error| format!("创建实况照片失败: {error}"))?;
    let mut writer = BufWriter::new(file);
    let mut write_part = |bytes: &[u8]| {
        writer
            .write_all(bytes)
            .map_err(|error| format!("写入实况照片失败: {error}"))
    };
    write_part(&jpeg[..2])?;
    write_part(&[0xFF, 0xE1])?;
    write_part(&((payload.len() + 2) as u16).to_be_bytes())?;
    write_part(&payload)?;
    write_part(&jpeg[2..])?;
    write_part(&video)?;
    writer
        .flush()
        .map_err(|error| format!("写入实况照片失败: {error}"))
}

fn as_jpeg(path: &Path, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.starts_with(&[0xFF, 0xD8]) {
        return Ok(data.to_vec());
    }
    if let Ok(image) = image::load_from_memory(data) {
        let mut output = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 95);
        image
            .write_with_encoder(encoder)
            .map_err(|error| format!("编码 JPEG 失败: {error}"))?;
        return Ok(output);
    }
    // HEIC/HEIF/AVIF 只能交给系统解码器。
    let converted_path = path.with_extension(format!("motion-{}.jpg", std::process::id()));
    let converted = crate::image_convert::convert_to_jpeg(path, &converted_path, 0, 95);
    let result = if converted {
        std::fs::read(&converted_path).map_err(|error| error.to_string())
    } else {
        Err("图片格式不受当前系统解码器支持".to_string())
    };
    let _ = std::fs::remove_file(&converted_path);
    let data = result.map_err(|error| format!("图片无法转换为 JPEG: {error}"))?;
    if data.is_empty() {
        return Err("图片无法转换为 JPEG".to_string());
    }
    Ok(data)
}

/// 去掉已有的 XMP APP1 段，避免一个文件出现多个 XMP 数据包。
fn strip_xmp_segments(jpeg: &[u8]) -> Vec<u8> {
    if jpeg.len() < 2 {
        return jpeg.to_vec();
    }
    let mut out = Vec::with_capacity(jpeg.len());
    out.extend_from_slice(&jpeg[..2]);
    let mut pos = 2;
    while pos + 4 <= jpeg.len() {
        if jpeg[pos] != 0xFF {
            break;
        }
        let marker = jpeg[pos + 1];
        // SOS 之后是熵编码数据，原样保留到文件结尾。
        if marker == 0xDA {
            out.extend_from_slice(&jpeg[pos..]);
            return out;
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            out.extend_from_slice(&jpeg[pos..pos + 2]);
            pos += 2;
            continue;
        }
        let length = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
        if length < 2 || pos + 2 + length > jpeg.len() {
            break;
        }
        let payload = &jpeg[pos + 4..pos + 2 + length];
        let is_xmp = marker == 0xE1 && payload.starts_with(XMP_IDENTIFIER);
        if !is_xmp {
            out.extend_from_slice(&jpeg[pos..pos + 2 + length]);
        }
        pos += 2 + length;
    }
    out.extend_from_slice(&jpeg[pos..]);
    out
}

/// 截断到 JPEG 的 EOI 标记，防止原图尾部的多余字节落到视频前面。
fn trim_to_eoi(jpeg: &[u8]) -> Result<&[u8], String> {
    let end = jpeg
        .windows(2)
        .rposition(|window| window == [0xFF, 0xD9])
        .map(|index| index + 2)
        .ok_or_else(|| "JPEG 数据缺少结尾标记".to_string())?;
    Ok(&jpeg[..end])
}

/// 主图像项 Length 按规范写 0，视频项 Length 用于从文件尾部定位视频。
fn build_xmp(video_length: usize) -> String {
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
<rdf:Description rdf:about=\"\" \
xmlns:GCamera=\"http://ns.google.com/photos/1.0/camera/\" \
xmlns:Container=\"http://ns.google.com/photos/1.0/container/\" \
xmlns:Item=\"http://ns.google.com/photos/1.0/container/item/\" \
GCamera:MotionPhoto=\"1\" \
GCamera:MotionPhotoVersion=\"1\" \
GCamera:MotionPhotoPresentationTimestampUs=\"0\">\
<Container:Directory><rdf:Seq>\
<rdf:li rdf:parseType=\"Resource\">\
<Container:Item Item:Mime=\"image/jpeg\" Item:Semantic=\"Primary\" Item:Length=\"0\" Item:Padding=\"0\"/>\
</rdf:li>\
<rdf:li rdf:parseType=\"Resource\">\
<Container:Item Item:Mime=\"video/mp4\" Item:Semantic=\"MotionPhoto\" Item:Length=\"{video_length}\" Item:Padding=\"0\"/>\
</rdf:li>\
</rdf:Seq></Container:Directory>\
</rdf:Description></rdf:RDF></x:xmpmeta>\
<?xpacket end=\"w\"?>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, ImageFormat, Rgb};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("reels_motion_{}_{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn sample_image(path: &Path, format: ImageFormat) {
        let image = ImageBuffer::from_pixel(4, 4, Rgb([120u8, 30, 200]));
        image
            .save_with_format(path, format)
            .expect("write sample image");
    }

    #[test]
    fn writes_xmp_and_appends_video() {
        let dir = temp_dir("jpeg");
        let image_path = dir.join("photo.jpg");
        let video_path = dir.join("live.mp4");
        let output_path = dir.join("photo.MP.jpg");
        sample_image(&image_path, ImageFormat::Jpeg);
        let video = b"\x00\x00\x00\x18ftypmp42fake-video".to_vec();
        std::fs::write(&video_path, &video).expect("write video");

        write(&image_path, &video_path, &output_path).expect("write motion photo");
        let data = std::fs::read(&output_path).expect("read result");
        assert!(data.starts_with(&[0xFF, 0xD8]));
        assert!(data.ends_with(&video));
        assert!(contains(&data, b"GCamera:MotionPhoto=\"1\""));
        assert!(contains(
            &data,
            format!("Item:Length=\"{}\"", video.len()).as_bytes()
        ));
        assert!(contains(&data, b"Item:Semantic=\"MotionPhoto\""));
        assert_eq!(
            data.windows(b"http://ns.adobe.com/xap/1.0/\0".len())
                .filter(|window| *window == b"http://ns.adobe.com/xap/1.0/\0")
                .count(),
            1
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn converts_non_jpeg_image() {
        let dir = temp_dir("png");
        let image_path = dir.join("photo.png");
        let video_path = dir.join("live.mp4");
        let output_path = dir.join("photo.MP.jpg");
        sample_image(&image_path, ImageFormat::Png);
        std::fs::write(&video_path, b"fake-video").expect("write video");

        write(&image_path, &video_path, &output_path).expect("write motion photo");
        let data = std::fs::read(&output_path).expect("read result");
        assert!(data.starts_with(&[0xFF, 0xD8]));
        assert!(data.ends_with(b"fake-video"));
        assert!(contains(&data, b"GCamera:MotionPhoto=\"1\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }
}
