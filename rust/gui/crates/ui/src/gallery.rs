//! 将移动端下载文件写入系统相册。

/// 返回是否已确认写入相册。iOS 端为异步提交，返回仅表示已发起保存。
pub fn save_media(path: &std::path::Path) -> bool {
    let Some(text) = path.to_str() else {
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
        return matches!(result, Ok(true));
    }

    #[cfg(target_os = "ios")]
    {
        if let Ok(c_path) = std::ffi::CString::new(text) {
            unsafe {
                extern "C" {
                    fn reels_save_media_to_photos(path: *const std::os::raw::c_char);
                }
                reels_save_media_to_photos(c_path.as_ptr());
            }
            // 保存完成后由原生层删除沙盒源文件，避免 Documents 双倍占用。
            return true;
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
