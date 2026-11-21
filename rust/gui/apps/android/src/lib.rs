//! Android Tauri entry point.

use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use douyin_core::extractor::{
    ExtractionResult, MediaEntry, MediaKind, MediaPlatform, VideoExtractor,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_plugin_clipboard_manager::ClipboardExt;

mod image_convert;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme_mode: String,
    pub last_directory: String,
    pub skip_location_prompt: bool,
    pub remember_location: bool,
    pub save_to_gallery: bool,
    pub xhs_cookie: String,
    pub live_photo_export: bool,
    pub result_view: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    pub size: f64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WindowInsets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub ime_bottom: f64,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub downloaded: f64,
    pub total: f64,
}

impl From<douyin_core::settings::AppSettings> for Settings {
    fn from(value: douyin_core::settings::AppSettings) -> Self {
        Self {
            theme_mode: value.theme_mode,
            last_directory: value.last_directory,
            skip_location_prompt: value.skip_location_prompt,
            remember_location: value.remember_location,
            save_to_gallery: value.save_to_gallery,
            xhs_cookie: value.xhs_cookie,
            live_photo_export: value.live_photo_export,
            result_view: value.result_view,
        }
    }
}

impl From<Settings> for douyin_core::settings::AppSettings {
    fn from(value: Settings) -> Self {
        Self {
            theme_mode: value.theme_mode,
            last_directory: value.last_directory,
            skip_location_prompt: value.skip_location_prompt,
            remember_location: value.remember_location,
            save_to_gallery: value.save_to_gallery,
            xhs_cookie: value.xhs_cookie,
            live_photo_export: value.live_photo_export,
            result_view: value.result_view,
        }
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub url: String,
    pub extension: Option<String>,
    pub duration_secs: Option<f64>,
    pub size_bytes: Option<f64>,
    pub quality: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub thumbnail_url: Option<String>,
    pub thumbnail_width: Option<f64>,
    pub thumbnail_height: Option<f64>,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedResult {
    pub success: bool,
    pub original_url: String,
    pub platform: String,
    pub media_kind: String,
    pub title: Option<String>,
    pub entries: Vec<MediaItem>,
    pub covers: Vec<MediaItem>,
    pub error: Option<String>,
}

fn media_item(entry: &MediaEntry) -> MediaItem {
    MediaItem {
        url: entry.url.clone(),
        extension: entry.extension.clone(),
        duration_secs: entry.duration_secs.map(|v| v as f64),
        size_bytes: entry.size_bytes.map(|v| v as f64),
        quality: entry.quality.clone(),
        width: entry.width.map(|v| v as f64),
        height: entry.height.map(|v| v as f64),
        thumbnail_url: entry.thumbnail_url.clone(),
        thumbnail_width: entry.thumbnail_width.map(|v| v as f64),
        thumbnail_height: entry.thumbnail_height.map(|v| v as f64),
        cover_url: entry.cover_url.clone(),
    }
}

fn extracted(result: &ExtractionResult) -> ExtractedResult {
    ExtractedResult {
        success: result.success,
        original_url: result.original_url.clone(),
        platform: match result.platform {
            MediaPlatform::Douyin => "douyin",
            MediaPlatform::Xhs => "xhs",
        }
        .into(),
        media_kind: match result.media_kind {
            MediaKind::Video => "video",
            MediaKind::Image => "image",
        }
        .into(),
        title: result.title.clone(),
        entries: result.entries.iter().map(media_item).collect(),
        covers: result.covers.iter().map(media_item).collect(),
        error: result.error.clone(),
    }
}

fn config_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    Ok(home.join(".config").join("reels"))
}

fn settings_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("settings.json"))
}

fn cache_path() -> Result<PathBuf, String> {
    Ok(config_dir()?.join("cache"))
}

fn settings_state() -> &'static Mutex<Option<douyin_core::settings::AppSettings>> {
    static SETTINGS: OnceLock<Mutex<Option<douyin_core::settings::AppSettings>>> = OnceLock::new();
    SETTINGS.get_or_init(|| Mutex::new(None))
}

#[tauri::command]
#[specta::specta]
fn load_settings() -> Result<Settings, String> {
    let mut guard = settings_state()
        .lock()
        .map_err(|_| "settings state".to_string())?;
    if guard.is_none() {
        *guard = Some(douyin_core::settings::AppSettings::load(&settings_path()?));
    }
    Ok(guard.clone().unwrap().into())
}

#[tauri::command]
#[specta::specta]
fn save_settings(settings: Settings) -> Result<(), String> {
    let settings: douyin_core::settings::AppSettings = settings.into();
    settings
        .save(&settings_path()?)
        .map_err(|e| e.to_string())?;
    *settings_state()
        .lock()
        .map_err(|_| "settings state".to_string())? = Some(settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn clipboard_read(app: tauri::AppHandle) -> Result<String, String> {
    app.clipboard().read_text().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
fn clipboard_write(app: tauri::AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

#[cfg(target_os = "android")]
fn main_activity_class<'local>(
    env: &mut jni::JNIEnv<'local>,
    context: &jni::objects::JObject<'local>,
) -> Result<jni::objects::JClass<'local>, String> {
    use jni::objects::JValue;
    let loader = env
        .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let name = env
        .new_string("com.dreamfly.reels.MainActivity")
        .map_err(|e| e.to_string())?;
    let class = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    Ok(jni::objects::JClass::from(class))
}

fn referer_for(url: &str) -> &'static str {
    if url.contains("xhscdn.com") || url.contains("xiaohongshu.com") {
        "https://www.xiaohongshu.com/"
    } else {
        "https://www.douyin.com/"
    }
}

static DOWNLOADED_BYTES: AtomicU64 = AtomicU64::new(0);
static DOWNLOAD_TOTAL_BYTES: AtomicU64 = AtomicU64::new(0);

fn download_to_file(url: &str, path: &Path) -> Result<(), String> {
    DOWNLOADED_BYTES.store(0, Ordering::Relaxed);
    DOWNLOAD_TOTAL_BYTES.store(0, Ordering::Relaxed);
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .header(reqwest::header::REFERER, referer_for(url))
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let total = response.content_length().unwrap_or(0);
    DOWNLOAD_TOTAL_BYTES.store(total, Ordering::Relaxed);
    let mut file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut downloaded = 0_u64;
    loop {
        let read = response.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        downloaded += read as u64;
        DOWNLOADED_BYTES.store(downloaded, Ordering::Relaxed);
    }
    file.flush().map_err(|e| e.to_string())?;
    if total > 0 {
        DOWNLOAD_TOTAL_BYTES.store(total, Ordering::Relaxed);
    } else {
        DOWNLOAD_TOTAL_BYTES.store(downloaded, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn download_progress() -> Result<DownloadProgress, String> {
    Ok(DownloadProgress {
        downloaded: DOWNLOADED_BYTES.load(Ordering::Relaxed) as f64,
        total: DOWNLOAD_TOTAL_BYTES.load(Ordering::Relaxed) as f64,
    })
}

#[tauri::command]
#[specta::specta]
fn thumbnail_data(url: String) -> Result<String, String> {
    let cache_dir = cache_path()?.join("thumbnails");
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    let cache_file = cache_dir.join(format!("{:016x}.bin", hasher.finish()));
    if let Ok(bytes) = fs::read(&cache_file) {
        if !bytes.is_empty() {
            return Ok(format!(
                "data:{};base64,{}",
                image_mime(&bytes),
                base64_encode(&bytes)
            ));
        }
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(&url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .header(reqwest::header::REFERER, referer_for(&url))
        .send()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response.bytes().map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("empty thumbnail".into());
    }
    if image_convert::needs_platform_decoder(&bytes) {
        let source = cache_file.with_extension("source");
        fs::write(&source, &bytes).map_err(|e| e.to_string())?;
        let converted = image_convert::convert_to_jpeg(&source, &cache_file, 2048, 90);
        let _ = fs::remove_file(&source);
        if converted {
            let jpeg = fs::read(&cache_file).map_err(|e| e.to_string())?;
            return Ok(format!("data:image/jpeg;base64,{}", base64_encode(&jpeg)));
        }
        return Err("当前设备无法解码该 HEIC/AVIF 图片".into());
    }
    let decoded = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let mut jpeg = Vec::new();
    image::DynamicImage::write_with_encoder(
        &decoded,
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90),
    )
    .map_err(|e| e.to_string())?;
    let _ = fs::write(&cache_file, &jpeg);
    Ok(format!("data:image/jpeg;base64,{}", base64_encode(&jpeg)))
}

fn image_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

#[tauri::command]
#[specta::specta]
fn window_insets() -> Result<WindowInsets, String> {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        let activity_class = main_activity_class(&mut env, &activity)?;
        let value = env
            .call_static_method(
                &activity_class,
                "safeAreaInsets",
                "(Landroid/content/Context;)Ljava/lang/String;",
                &[JValue::Object(&activity)],
            )
            .map_err(|e| e.to_string())?
            .l()
            .map_err(|e| e.to_string())?;
        let value: jni::objects::JString = value.into();
        let value = env
            .get_string(&value)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        let values: Vec<f64> = value
            .split(',')
            .filter_map(|part| part.trim().parse::<f64>().ok())
            .collect();
        if values.len() == 5 {
            return Ok(WindowInsets {
                left: values[0],
                top: values[1],
                right: values[2],
                bottom: values[3],
                ime_bottom: values[4],
            });
        }
        Err("invalid Android window insets".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(WindowInsets {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            ime_bottom: 0.0,
        })
    }
}

#[tauri::command]
#[specta::specta]
fn set_system_bars(dark: bool) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        let activity_class = main_activity_class(&mut env, &activity)?;
        env.call_static_method(
            &activity_class,
            "setSystemBars",
            "(Landroid/content/Context;Z)V",
            &[JValue::Object(&activity), JValue::Bool(dark as u8)],
        )
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "android"))]
    let _ = dark;
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn exit_app() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        let activity_class = main_activity_class(&mut env, &activity)?;
        env.call_static_method(
            &activity_class,
            "exitApp",
            "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn android_ready() -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        let activity_class = main_activity_class(&mut env, &activity)?;
        env.call_static_method(
            &activity_class,
            "hideStartupSplash",
            "(Landroid/content/Context;)V",
            &[JValue::Object(&activity)],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn parse_links(text: String) -> Result<Vec<ExtractedResult>, String> {
    let settings = load_settings()?;
    let cookie = (!settings.xhs_cookie.trim().is_empty()).then_some(settings.xhs_cookie);
    let extractor = VideoExtractor::new()
        .map_err(|e| e.to_string())?
        .with_xhs_cookie(cookie);
    Ok(extractor
        .extract_multiple(&text)
        .iter()
        .map(extracted)
        .collect())
}

#[tauri::command]
#[specta::specta]
fn cache_info() -> Result<CacheInfo, String> {
    let path = cache_path()?;
    let size = dir_size(&path) as f64;
    Ok(CacheInfo {
        size,
        path: format!("下载缓存目录: {}", path.display()),
    })
}

#[tauri::command]
#[specta::specta]
fn clear_cache() -> Result<(), String> {
    let path = cache_path()?;
    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
fn app_version() -> String {
    option_env!("APP_VERSION")
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

#[tauri::command]
#[specta::specta]
fn save_media(path: String, live_path: Option<String>) -> Result<bool, String> {
    let path = PathBuf::from(path);
    let Some(live) = live_path.map(PathBuf::from) else {
        return save_to_gallery(&path);
    };
    let output = path.with_extension("MP.jpg");
    motion_photo::write(&path, &live, &output)?;
    let saved = save_to_gallery(&output)?;
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(live);
    let _ = fs::remove_file(output);
    Ok(saved)
}

#[tauri::command]
#[specta::specta]
async fn download_media(
    url: String,
    extension: Option<String>,
    live_url: Option<String>,
) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || download_media_blocking(url, extension, live_url))
        .await
        .map_err(|error| format!("download task failed: {error}"))?
}

fn download_media_blocking(
    url: String,
    extension: Option<String>,
    live_url: Option<String>,
) -> Result<bool, String> {
    let dir = cache_path()?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let suffix = extension
        .unwrap_or_else(|| "mp4".into())
        .replace(['/', '\\'], "_");
    let path = dir.join(format!("reels-{stamp}.{suffix}"));
    download_to_file(&url, &path)?;
    if let Some(live_url) = live_url {
        let live_path = dir.join(format!("reels-{stamp}-live.mp4"));
        download_to_file(&live_url, &live_path)?;
        let saved = save_media(
            path.to_string_lossy().into(),
            Some(live_path.to_string_lossy().into()),
        );
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&live_path);
        return saved;
    }
    let saved = save_to_gallery(&path)?;
    if saved {
        let _ = fs::remove_file(&path);
    }
    Ok(saved)
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(target_os = "android")]
fn save_to_gallery(path: &Path) -> Result<bool, String> {
    use jni::objects::JValue;
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let activity_class = main_activity_class(&mut env, &activity)?;
    let value = env
        .new_string(path.to_string_lossy().as_ref())
        .map_err(|e| e.to_string())?;
    Ok(env
        .call_static_method(
            &activity_class,
            "saveMediaToGalleryFromContext",
            "(Landroid/content/Context;Ljava/lang/String;)Z",
            &[JValue::Object(&activity), JValue::Object(&value)],
        )
        .map_err(|e| e.to_string())?
        .z()
        .map_err(|e| e.to_string())?)
}

#[cfg(not(target_os = "android"))]
fn save_to_gallery(_path: &Path) -> Result<bool, String> {
    Ok(false)
}

mod motion_photo {
    use std::fs::File;
    use std::io::{BufWriter, Write};
    use std::path::Path;

    use super::*;

    const XMP_IDENTIFIER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

    pub fn write(image: &Path, video: &Path, output: &Path) -> Result<(), String> {
        let video = fs::read(video).map_err(|e| e.to_string())?;
        if video.is_empty() {
            return Err("实况视频为空".to_string());
        }
        let image_data = fs::read(image).map_err(|e| e.to_string())?;
        let jpeg = as_jpeg(image, &image_data)?;
        let stripped = strip_xmp_segments(&jpeg);
        let jpeg = trim_to_eoi(&stripped)?;
        let mut payload = XMP_IDENTIFIER.to_vec();
        payload.extend_from_slice(build_xmp(video.len()).as_bytes());
        if payload.len() + 2 > u16::MAX as usize {
            return Err("XMP 元数据过大".to_string());
        }

        let file = File::create(output).map_err(|e| e.to_string())?;
        let mut writer = BufWriter::new(file);
        let mut write_part = |bytes: &[u8]| writer.write_all(bytes).map_err(|e| e.to_string());
        write_part(&jpeg[..2])?;
        write_part(&[0xFF, 0xE1])?;
        write_part(&((payload.len() + 2) as u16).to_be_bytes())?;
        write_part(&payload)?;
        write_part(&jpeg[2..])?;
        write_part(&video)?;
        writer.flush().map_err(|e| e.to_string())
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
                .map_err(|e| e.to_string())?;
            return Ok(output);
        }
        let converted_path = path.with_extension(format!("motion-{}.jpg", std::process::id()));
        let converted = crate::image_convert::convert_to_jpeg(path, &converted_path, 0, 95);
        let result = if converted {
            fs::read(&converted_path).map_err(|e| e.to_string())
        } else {
            Err("图片格式不受当前系统解码器支持".to_string())
        };
        let _ = fs::remove_file(&converted_path);
        result
    }

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

    fn trim_to_eoi(jpeg: &[u8]) -> Result<&[u8], String> {
        let end = jpeg
            .windows(2)
            .rposition(|window| window == [0xFF, 0xD9])
            .map(|index| index + 2)
            .ok_or_else(|| "JPEG 数据缺少结尾标记".to_string())?;
        Ok(&jpeg[..end])
    }

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
}

pub fn builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .error_handling(tauri_specta::ErrorHandlingMode::Result)
        .commands(tauri_specta::collect_commands![
            load_settings,
            save_settings,
            parse_links,
            cache_info,
            clear_cache,
            app_version,
            save_media,
            download_media,
            clipboard_read,
            clipboard_write,
            thumbnail_data,
            window_insets,
            set_system_bars,
            exit_app,
            download_progress,
            android_ready,
        ])
}

fn export_bindings() {
    builder()
        .export(
            specta_typescript::Typescript::default(),
            Path::new("frontend/src/bindings.ts"),
        )
        .expect("failed to export TypeScript bindings");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bindings_to_disk() {
        export_bindings();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = builder();
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("Tauri application failed");
}
