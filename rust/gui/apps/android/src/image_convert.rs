//! HEIC/HEIF decoding with the Android platform decoder and a Rust fallback.

use std::path::Path;

pub fn needs_platform_decoder(data: &[u8]) -> bool {
    if data.len() < 12 || &data[4..8] != b"ftyp" {
        return false;
    }
    matches!(
        &data[8..12],
        b"heic"
            | b"heix"
            | b"hevc"
            | b"hevx"
            | b"heim"
            | b"heis"
            | b"hevm"
            | b"hevs"
            | b"mif1"
            | b"msf1"
            | b"avif"
            | b"avis"
    )
}

pub fn convert_to_jpeg(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
    #[cfg(target_os = "android")]
    {
        android::convert(source, target, max_dimension, quality)
            || builtin_heif::convert(source, target, max_dimension, quality)
    }
    #[cfg(not(target_os = "android"))]
    {
        builtin_heif::convert(source, target, max_dimension, quality)
    }
}

mod builtin_heif {
    use std::io::BufWriter;
    use std::path::Path;

    use image::imageops::FilterType;

    pub fn convert(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
        let Ok(data) = std::fs::read(source) else {
            return false;
        };
        let Ok(decoded) = heic_decoder::decode(&data) else {
            return false;
        };
        let Ok(rgb) = decoded.to_rgb8() else {
            return false;
        };
        let Some(image) = image::RgbImage::from_raw(rgb.width as u32, rgb.height as u32, rgb.data)
        else {
            return false;
        };

        let longest = image.width().max(image.height());
        let image = if max_dimension > 0 && longest > max_dimension {
            let ratio = max_dimension as f64 / longest as f64;
            let width = ((image.width() as f64 * ratio).round() as u32).max(1);
            let height = ((image.height() as f64 * ratio).round() as u32).max(1);
            image::imageops::resize(&image, width, height, FilterType::Triangle)
        } else {
            image
        };

        let Ok(file) = std::fs::File::create(target) else {
            return false;
        };
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
            BufWriter::new(file),
            quality.clamp(1, 100),
        );
        match encoder.encode_image(&image) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("failed to encode HEIC as JPEG: {error}");
                let _ = std::fs::remove_file(target);
                false
            }
        }
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::path::Path;

    use jni::objects::JValue;

    pub fn convert(source: &Path, target: &Path, max_dimension: u32, quality: u8) -> bool {
        let (Some(source), Some(target)) = (source.to_str(), target.to_str()) else {
            return false;
        };
        let ctx = ndk_context::android_context();
        let Ok(vm) = (unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }) else {
            return false;
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            return false;
        };
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        let Ok(class) = super::super::main_activity_class(&mut env, &activity) else {
            return false;
        };
        let Ok(source) = env.new_string(source) else {
            return false;
        };
        let Ok(target) = env.new_string(target) else {
            return false;
        };
        env.call_static_method(
            &class,
            "convertImageToJpegFromContext",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;II)Z",
            &[
                JValue::Object(&activity),
                JValue::Object(&source),
                JValue::Object(&target),
                JValue::Int(max_dimension as i32),
                JValue::Int(quality as i32),
            ],
        )
        .and_then(|result| result.z())
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_heic_brands() {
        let mut heic = vec![0u8; 16];
        heic[4..8].copy_from_slice(b"ftyp");
        heic[8..12].copy_from_slice(b"heic");
        assert!(needs_platform_decoder(&heic));
    }

    #[test]
    fn ignores_jpeg() {
        assert!(!needs_platform_decoder(&[0xFF, 0xD8, 0xFF, 0xE0]));
    }
}
