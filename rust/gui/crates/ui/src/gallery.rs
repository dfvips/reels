//! 将移动端下载文件写入系统相册。

/// 相册对 WebP 兼容性差（部分版本会保存失败或显示异常），
/// 保存前按原始尺寸转成 JPEG，返回临时文件路径。
#[cfg(any(target_os = "ios", target_os = "android"))]
fn webp_as_jpeg(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }
    let image = image::load_from_memory_with_format(&data, image::ImageFormat::WebP).ok()?;
    let output = path.with_extension("jpg");
    let file = std::fs::File::create(&output).ok()?;
    let mut writer = std::io::BufWriter::new(file);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 92);
    image.write_with_encoder(encoder).ok()?;
    Some(output)
}

/// 返回是否已确认写入相册。iOS 端为异步提交，返回仅表示已发起保存。
pub fn save_media(path: &std::path::Path) -> bool {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let converted = webp_as_jpeg(path);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let converted: Option<std::path::PathBuf> = None;

    let target = converted.as_deref().unwrap_or(path);
    let Some(text) = target.to_str() else {
        if let Some(converted) = converted.as_deref() {
            let _ = std::fs::remove_file(converted);
        }
        return false;
    };
    #[cfg(target_os = "android")]
    {
        use gpui_android::android::jni as android_jni;
        use jni::objects::JValue;

        let result = android_jni::with_env(|env| {
            let activity = android_jni::activity(env)?;
            let value = env.new_string(text).map_err(|error| error.to_string())?;
            env.call_method(
                &activity,
                jni::jni_str!("saveMediaToGallery"),
                jni::jni_sig!("(Ljava/lang/String;)Z"),
                &[JValue::Object(&value)],
            )
            .and_then(|result| result.z())
            .map_err(|error| error.to_string())
        });
        if let Err(error) = &result {
            eprintln!("failed to save media to Android gallery: {error}");
        }
        if let Some(converted) = converted.as_deref() {
            let _ = std::fs::remove_file(converted);
        }
        return matches!(result, Ok(true));
    }

    #[cfg(target_os = "ios")]
    {
        if let Ok(c_path) = std::ffi::CString::new(text) {
            if converted.is_some() {
                // 原生层保存后删除传入的 JPG，源 WebP 由这里清理。
                let _ = std::fs::remove_file(path);
            }
            unsafe {
                extern "C" {
                    fn reels_save_media_to_photos(path: *const std::os::raw::c_char);
                }
                reels_save_media_to_photos(c_path.as_ptr());
            }
            // 保存完成后由原生层删除沙盒源文件，避免 Documents 双倍占用。
            return true;
        }
        if let Some(converted) = converted.as_deref() {
            let _ = std::fs::remove_file(converted);
        }
        return false;
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = text;
    true
}

/// 保存成功后是否由调用方删除沙盒源文件。
/// iOS 的相册提交是异步的，由原生层在完成后删除；调用方提前删除会导致保存失败。
pub fn caller_deletes_source_after_save() -> bool {
    cfg!(target_os = "android")
}

/// 将照片与动轨封装为实况照片写入相册（图片高质量重编码以写入标识，视频不转码）。
/// 返回是否已提交保存请求；iOS 原生层在完成后删除两个源文件。
pub fn save_live_photo(image: &std::path::Path, video: &std::path::Path) -> bool {
    #[cfg(target_os = "ios")]
    {
        let (Some(image), Some(video)) = (image.to_str(), video.to_str()) else {
            return false;
        };
        let (Ok(image), Ok(video)) = (std::ffi::CString::new(image), std::ffi::CString::new(video))
        else {
            return false;
        };
        unsafe {
            extern "C" {
                fn reels_save_live_photo_to_photos(
                    image_path: *const std::os::raw::c_char,
                    video_path: *const std::os::raw::c_char,
                ) -> bool;
            }
            reels_save_live_photo_to_photos(image.as_ptr(), video.as_ptr())
        }
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (image, video);
        false
    }
}
