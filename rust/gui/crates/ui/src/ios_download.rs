//! iOS 原生后台下载桥接。
//!
//! 传输由 Swift 侧的 `URLSessionConfiguration.background` 完成，Rust 仅负责
//! 生成下载计划、接收事件并在后台传输结束后交给相册保存逻辑。

#![cfg(target_os = "ios")]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::download::DownloadEvent;

struct ActiveBatch {
    id: String,
    tx: Sender<DownloadEvent>,
}

static ACTIVE: OnceLock<Mutex<Option<ActiveBatch>>> = OnceLock::new();
static BATCH_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

unsafe extern "C" {
    fn reels_ios_background_download_start(
        batch_id: *const c_char,
        urls: *const *const c_char,
        titles: *const *const c_char,
        extensions: *const *const c_char,
        live_videos: *const *const c_char,
        sizes: *const u64,
        count: usize,
        directory: *const c_char,
        cache_dir: *const c_char,
    ) -> bool;
    fn reels_ios_background_download_cancel(batch_id: *const c_char);
}

fn next_batch_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let sequence = BATCH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("reels-{millis}-{sequence}")
}

fn required_strings(values: &[String]) -> Option<(Vec<CString>, Vec<*const c_char>)> {
    let strings = values
        .iter()
        .map(|value| CString::new(value.as_bytes()).ok())
        .collect::<Option<Vec<_>>>()?;
    let pointers = strings.iter().map(|value| value.as_ptr()).collect();
    Some((strings, pointers))
}

fn optional_strings(
    values: &[Option<String>],
) -> Option<(Vec<Option<CString>>, Vec<*const c_char>)> {
    let mut strings = Vec::with_capacity(values.len());
    for value in values {
        let value = match value {
            Some(value) => Some(CString::new(value.as_bytes()).ok()?),
            None => None,
        };
        strings.push(value);
    }
    let pointers = strings
        .iter()
        .map(|value| {
            value
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr())
        })
        .collect();
    Some((strings, pointers))
}

pub(crate) fn start(
    urls: &[String],
    titles: &[String],
    extensions: &[Option<String>],
    live_videos: &[Option<String>],
    sizes: &[Option<u64>],
    directory: &Path,
    cache_dir: &Path,
    tx: Sender<DownloadEvent>,
) -> bool {
    if urls.is_empty() {
        return false;
    }
    // CString 的底层缓冲区必须活到 Swift 调用返回，不能用 `_` 丢弃。
    let Some((id_string, _)) = required_strings(&[next_batch_id()]) else {
        return false;
    };
    let Some((url_strings, url_pointers)) = required_strings(urls) else {
        return false;
    };
    let Some((title_strings, title_pointers)) = required_strings(titles) else {
        return false;
    };
    let Some((extension_strings, extension_pointers)) = optional_strings(extensions) else {
        return false;
    };
    let Some((live_strings, live_pointers)) = optional_strings(live_videos) else {
        return false;
    };
    let (Ok(directory), Ok(cache_dir)) = (
        CString::new(directory.to_string_lossy().as_bytes()),
        CString::new(cache_dir.to_string_lossy().as_bytes()),
    ) else {
        return false;
    };
    let sizes = urls
        .iter()
        .enumerate()
        .map(|(index, _)| sizes.get(index).and_then(|size| *size).unwrap_or(0))
        .collect::<Vec<_>>();
    let batch_id = id_string[0].to_string_lossy().into_owned();

    *ACTIVE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(ActiveBatch {
        id: batch_id.clone(),
        tx,
    });

    let started = unsafe {
        reels_ios_background_download_start(
            id_string[0].as_ptr(),
            url_pointers.as_ptr(),
            title_pointers.as_ptr(),
            extension_pointers.as_ptr(),
            live_pointers.as_ptr(),
            sizes.as_ptr(),
            url_pointers.len(),
            directory.as_ptr(),
            cache_dir.as_ptr(),
        )
    };
    drop((
        url_strings,
        title_strings,
        extension_strings,
        live_strings,
        sizes,
    ));
    if !started {
        *ACTIVE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
    started
}

pub(crate) fn cancel_active() {
    let Some(batch) = ACTIVE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|batch| batch.id.clone())
    else {
        return;
    };
    if let Ok(batch) = CString::new(batch) {
        unsafe {
            reels_ios_background_download_cancel(batch.as_ptr());
        }
    }
}

fn with_active(batch_id: Option<&CStr>, event: impl FnOnce(&Sender<DownloadEvent>)) {
    let Some(batch_id) = batch_id.and_then(|value| value.to_str().ok()) else {
        return;
    };
    let active = ACTIVE.get_or_init(|| Mutex::new(None));
    let guard = active
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(batch) = guard.as_ref().filter(|batch| batch.id == batch_id) else {
        return;
    };
    event(&batch.tx);
}

fn path_from_c(value: *const c_char) -> Option<PathBuf> {
    if value.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .ok()
        .map(PathBuf::from)
}

#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_finalize_downloaded_file(
    path: *const c_char,
    live_path: *const c_char,
) -> bool {
    let Some(path) = path_from_c(path) else {
        return false;
    };
    let live_path = path_from_c(live_path);
    std::panic::catch_unwind(|| {
        if let Some(live_path) = live_path.as_deref() {
            if crate::gallery::save_live_photo_and_wait(&path, live_path) {
                return true;
            }
            let _ = std::fs::remove_file(live_path);
        }
        crate::gallery::save_media_and_wait(&path)
    })
    .unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_download_event_started(
    batch_id: *const c_char,
    index: u32,
    total: u64,
) {
    if batch_id.is_null() {
        return;
    }
    let batch_id = unsafe { CStr::from_ptr(batch_id) };
    with_active(Some(batch_id), |tx| {
        let _ = tx.send(DownloadEvent::Started {
            index: index as usize,
            total,
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_download_event_progress(
    batch_id: *const c_char,
    index: u32,
    downloaded: u64,
    total: u64,
) {
    if batch_id.is_null() {
        return;
    }
    let batch_id = unsafe { CStr::from_ptr(batch_id) };
    with_active(Some(batch_id), |tx| {
        let _ = tx.send(DownloadEvent::Progress {
            index: index as usize,
            downloaded,
            total,
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_download_event_completed(
    batch_id: *const c_char,
    index: u32,
    path: *const c_char,
) {
    if batch_id.is_null() {
        return;
    }
    let batch_id = unsafe { CStr::from_ptr(batch_id) };
    let path = path_from_c(path).unwrap_or_default();
    with_active(Some(batch_id), |tx| {
        let _ = tx.send(DownloadEvent::Completed {
            index: index as usize,
            path,
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_download_event_failed(
    batch_id: *const c_char,
    index: u32,
    message: *const c_char,
    timed_out: bool,
) {
    if batch_id.is_null() {
        return;
    }
    let batch_id = unsafe { CStr::from_ptr(batch_id) };
    let message = if message.is_null() {
        "下载失败".to_string()
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    };
    with_active(Some(batch_id), |tx| {
        let _ = tx.send(DownloadEvent::Failed {
            index: index as usize,
            error: message,
            timed_out,
        });
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_download_event_finished(batch_id: *const c_char) {
    if batch_id.is_null() {
        return;
    }
    let batch_id = unsafe { CStr::from_ptr(batch_id) };
    let active = ACTIVE.get_or_init(|| Mutex::new(None));
    let mut guard = active
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(id) = batch_id.to_str().ok() else {
        return;
    };
    let Some(batch) = guard.as_ref().filter(|batch| batch.id == id) else {
        return;
    };
    let _ = batch.tx.send(DownloadEvent::Finished);
    *guard = None;
}
