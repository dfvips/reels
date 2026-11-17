//! 视频下载（后台线程 + 进度事件）。

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use douyin_core::files::{safe_file_name, unique_file_name};

use crate::paths::is_desktop;

#[derive(Debug)]
pub enum DownloadEvent {
    Started {
        index: usize,
        total: u64,
    },
    Progress {
        index: usize,
        downloaded: u64,
        total: u64,
    },
    Completed {
        index: usize,
        path: PathBuf,
    },
    Failed {
        index: usize,
        error: String,
        timed_out: bool,
    },
    Finished,
}

pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const PARALLEL_THRESHOLD_BYTES: u64 = 50 * 1024 * 1024;
const PARALLEL_SEGMENT_COUNT: u64 = 6;
/// 链接超过该时间没有任何进展（连接、响应头或读取数据）就判定为卡死并跳过。
const STALL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
struct DownloadError {
    message: String,
    timed_out: bool,
}

impl DownloadError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            timed_out: false,
        }
    }

    fn timeout() -> Self {
        Self {
            message: "下载超时（超过 10 秒无响应）".to_string(),
            timed_out: true,
        }
    }

    fn from_reqwest(context: &str, error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::timeout()
        } else {
            Self::new(format!("{context}: {error}"))
        }
    }

    fn from_io(context: &str, error: std::io::Error) -> Self {
        // 阻塞版 reqwest 把读取超时包装成 io::Error，原始类型藏在 source 里。
        let timed_out = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<reqwest::Error>())
            .is_some_and(reqwest::Error::is_timeout);
        if timed_out {
            Self::timeout()
        } else {
            Self::new(format!("{context}: {error}"))
        }
    }
}

pub fn start_downloads(
    urls: Vec<String>,
    titles: Vec<String>,
    extensions: Vec<Option<String>>,
    live_videos: Vec<Option<String>>,
    directory: PathBuf,
    cache_dir: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: Sender<DownloadEvent>,
) {
    std::thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder()
            .connect_timeout(STALL_TIMEOUT)
            // 阻塞版 reqwest 的 timeout 按单次读写（及等待响应头）计时，
            // 因此只限制“停滞”，不会截断长时间的大文件下载。
            .timeout(STALL_TIMEOUT)
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                let _ = tx.send(DownloadEvent::Failed {
                    index: 0,
                    error: format!("初始化下载失败: {error}"),
                    timed_out: false,
                });
                let _ = tx.send(DownloadEvent::Finished);
                return;
            }
        };

        let _ = std::fs::create_dir_all(&directory);
        let _ = std::fs::create_dir_all(&cache_dir);

        for (index, url) in urls.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let title = titles
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("抖音视频_{}", index + 1));
            let mut base = {
                let safe = safe_file_name(&title, 80, 180);
                if safe.trim().is_empty() {
                    format!("抖音视频_{}", index + 1)
                } else {
                    safe
                }
            };
            let extension = extensions
                .get(index)
                .cloned()
                .flatten()
                .unwrap_or_else(|| "mp4".to_string());
            let extension_suffix = format!(".{extension}");
            if base.to_ascii_lowercase().ends_with(&extension_suffix) {
                let suffix_len = extension_suffix.len();
                base.truncate(base.len() - suffix_len);
                base = base.trim_end_matches([' ', '.']).to_string();
            } else if base.to_ascii_lowercase().ends_with(".mp4") {
                base.truncate(base.len() - 4);
                base = base.trim_end_matches([' ', '.']).to_string();
            }
            if base.is_empty() {
                base = format!("抖音视频_{}", index + 1);
            }
            let file_name = download_file_name(&directory, &base, &extension);
            let destination = directory.join(&file_name);
            let temp_path = cache_dir.join(format!("{file_name}.part"));

            match download_one(&client, url, &temp_path, &extension, index, &cancel, &tx) {
                Ok(()) => {
                    let live_path = match live_videos.get(index).cloned().flatten() {
                        Some(companion_url) => {
                            match download_live_companion(
                                &client,
                                &companion_url,
                                &cache_dir,
                                &file_name,
                                index,
                                &cancel,
                                &tx,
                            ) {
                                Ok(path) => Some(path),
                                Err(error) => {
                                    if cancel.load(Ordering::Relaxed) {
                                        let _ = std::fs::remove_file(&temp_path);
                                        let _ = tx.send(DownloadEvent::Failed {
                                            index,
                                            error: error.message,
                                            timed_out: error.timed_out,
                                        });
                                        continue;
                                    }
                                    // 动轨下载失败时仍保存照片，避免整条丢失。
                                    eprintln!(
                                        "实况动轨下载失败，改为仅保存照片: {}",
                                        error.message
                                    );
                                    None
                                }
                            }
                        }
                        None => None,
                    };
                    let _ = std::fs::create_dir_all(&directory);
                    match move_into_place(&temp_path, &destination) {
                        Ok(()) => {
                            #[cfg(target_os = "ios")]
                            let completed_path = destination.clone();
                            #[cfg(not(target_os = "ios"))]
                            let mut completed_path = destination.clone();
                            let saved_to_gallery = if let Some(live_path) = live_path.as_deref() {
                                #[cfg(target_os = "ios")]
                                {
                                    let saved =
                                        crate::gallery::save_live_photo(&destination, live_path);
                                    if saved {
                                        true
                                    } else {
                                        let _ = std::fs::remove_file(live_path);
                                        crate::gallery::save_media(&destination)
                                    }
                                }
                                #[cfg(not(target_os = "ios"))]
                                {
                                    let (path, saved) = finalize_live_download(
                                        &destination,
                                        live_path,
                                        &directory,
                                        &cache_dir,
                                        &file_name,
                                    );
                                    completed_path = path;
                                    saved
                                }
                            } else {
                                crate::gallery::save_media(&destination)
                            };
                            if crate::gallery::caller_deletes_source_after_save()
                                && saved_to_gallery
                            {
                                // 相册已保存副本，删除沙盒内文件避免重复占用存储。
                                let _ = std::fs::remove_file(&destination);
                            }
                            let _ = tx.send(DownloadEvent::Completed {
                                index,
                                path: completed_path,
                            });
                        }
                        Err(error) => {
                            if let Some(live_path) = live_path.as_deref() {
                                let _ = std::fs::remove_file(live_path);
                            }
                            let _ = std::fs::remove_file(&temp_path);
                            let _ = tx.send(DownloadEvent::Failed {
                                index,
                                error,
                                timed_out: false,
                            });
                        }
                    }
                }
                Err(error) => {
                    let _ = tx.send(DownloadEvent::Failed {
                        index,
                        error: error.message,
                        timed_out: error.timed_out,
                    });
                }
            }
        }

        let _ = tx.send(DownloadEvent::Finished);
    });
}

fn move_into_place(temp_path: &Path, destination: &Path) -> Result<(), String> {
    match std::fs::rename(temp_path, destination) {
        Ok(()) => Ok(()),
        Err(rename_error) => match std::fs::copy(temp_path, destination) {
            Ok(_) => {
                let _ = std::fs::remove_file(temp_path);
                Ok(())
            }
            Err(copy_error) => Err(format!(
                "保存文件失败: 移动失败: {rename_error}; 复制失败: {copy_error}"
            )),
        },
    }
}

/// Android / Windows：把图片与动轨合成为 Google Motion Photo。
/// 合成失败时回退为分别保存图片和视频，避免丢失动轨。
fn finalize_live_download(
    destination: &Path,
    live_path: &Path,
    directory: &Path,
    cache_dir: &Path,
    file_name: &str,
) -> (PathBuf, bool) {
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    let motion_name = format!(
        "{}.jpg",
        unique_file_name(directory, &format!("{stem}.MP"), "jpg")
    );
    let motion_path = directory.join(&motion_name);
    let temp_path = cache_dir.join(format!("{motion_name}.part"));

    let assembled = crate::motion_photo::write(destination, live_path, &temp_path)
        .and_then(|()| move_into_place(&temp_path, &motion_path));
    if let Err(error) = assembled {
        eprintln!("实况照片合成失败，改为分别保存图片和视频: {error}");
        let _ = std::fs::remove_file(&temp_path);
        return fallback_live_download(destination, live_path, directory, stem);
    }

    let _ = std::fs::remove_file(destination);
    let _ = std::fs::remove_file(live_path);
    let saved_to_gallery = if is_desktop() {
        true
    } else {
        let saved = crate::gallery::save_media(&motion_path);
        if saved && crate::gallery::caller_deletes_source_after_save() {
            let _ = std::fs::remove_file(&motion_path);
        }
        saved
    };
    (motion_path, saved_to_gallery)
}

fn fallback_live_download(
    destination: &Path,
    live_path: &Path,
    directory: &Path,
    stem: &str,
) -> (PathBuf, bool) {
    if is_desktop() {
        let video_name = format!(
            "{}.mp4",
            unique_file_name(directory, &format!("{stem}-live"), "mp4")
        );
        let video_path = directory.join(video_name);
        if std::fs::rename(live_path, &video_path).is_err()
            && std::fs::copy(live_path, &video_path).is_ok()
        {
            let _ = std::fs::remove_file(live_path);
        }
        return (destination.to_path_buf(), true);
    }
    let photo_saved = crate::gallery::save_media(destination);
    let video_saved = crate::gallery::save_media(live_path);
    if crate::gallery::caller_deletes_source_after_save() {
        if photo_saved {
            let _ = std::fs::remove_file(destination);
        }
        if video_saved {
            let _ = std::fs::remove_file(live_path);
        }
    }
    (destination.to_path_buf(), photo_saved)
}

#[allow(clippy::too_many_arguments)]
fn download_live_companion(
    client: &reqwest::blocking::Client,
    url: &str,
    cache_dir: &Path,
    file_name: &str,
    index: usize,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<DownloadEvent>,
) -> Result<PathBuf, DownloadError> {
    let companion_name = format!("{file_name}-live.mp4");
    let temp_path = cache_dir.join(format!("{companion_name}.part"));
    download_one(client, url, &temp_path, "mp4", index, cancel, tx)?;
    let destination = cache_dir.join(&companion_name);
    let _ = std::fs::remove_file(&destination);
    match std::fs::rename(&temp_path, &destination) {
        Ok(()) => Ok(destination),
        Err(rename_error) => match std::fs::copy(&temp_path, &destination) {
            Ok(_) => {
                let _ = std::fs::remove_file(&temp_path);
                Ok(destination)
            }
            Err(copy_error) => {
                let _ = std::fs::remove_file(&temp_path);
                Err(DownloadError::new(format!(
                    "保存实况视频失败: 移动失败: {rename_error}; 复制失败: {copy_error}"
                )))
            }
        },
    }
}

fn download_file_name(directory: &Path, base: &str, extension: &str) -> String {
    format!(
        "{}.{}",
        unique_file_name(directory, base, extension),
        extension
    )
}

fn download_one(
    client: &reqwest::blocking::Client,
    url: &str,
    temp_path: &Path,
    extension: &str,
    index: usize,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<DownloadEvent>,
) -> Result<(), DownloadError> {
    if is_video_extension(extension) {
        if let Some(total) = probe_range_total(client, url) {
            if total > PARALLEL_THRESHOLD_BYTES {
                return download_parallel(client, url, temp_path, total, index, cancel, tx);
            }
        }
    }

    let mut response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, referer_for(url))
        .send()
        .map_err(|error| DownloadError::from_reqwest("下载请求失败", error))?
        .error_for_status()
        .map_err(|error| DownloadError::from_reqwest("服务器返回错误", error))?;

    let total = response.content_length().unwrap_or(0);
    let _ = tx.send(DownloadEvent::Started { index, total });

    let mut file = std::fs::File::create(temp_path)
        .map_err(|error| DownloadError::new(format!("创建临时文件失败: {error}")))?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut downloaded = 0u64;
    let mut last_reported = 0u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::new("下载已取消"));
        }
        let read = response
            .read(&mut buffer)
            .map_err(|error| DownloadError::from_io("读取数据失败", error))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| DownloadError::new(format!("写入文件失败: {error}")))?;
        downloaded += read as u64;
        if downloaded - last_reported >= 128 * 1024 || downloaded == total {
            last_reported = downloaded;
            let _ = tx.send(DownloadEvent::Progress {
                index,
                downloaded,
                total,
            });
        }
    }

    file.flush().ok();
    let _ = tx.send(DownloadEvent::Progress {
        index,
        downloaded,
        total: total.max(downloaded),
    });
    Ok(())
}

fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "mp4" | "mov" | "m4v" | "webm"
    )
}

fn probe_range_total(client: &reqwest::blocking::Client, url: &str) -> Option<u64> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, referer_for(url))
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .ok()?;
    if response.status().as_u16() != 206 {
        return None;
    }
    let content_range = response.headers().get(reqwest::header::CONTENT_RANGE)?;
    let content_range = content_range.to_str().ok()?;
    content_range.rsplit('/').next()?.trim().parse().ok()
}

fn download_parallel(
    client: &reqwest::blocking::Client,
    url: &str,
    temp_path: &Path,
    total: u64,
    index: usize,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<DownloadEvent>,
) -> Result<(), DownloadError> {
    let segment_count = PARALLEL_SEGMENT_COUNT.min(total);
    let segment_size = total / segment_count;
    let progress = Arc::new(Mutex::new(vec![0u64; segment_count as usize]));
    let abort = Arc::new(AtomicBool::new(false));
    let _ = tx.send(DownloadEvent::Started { index, total });

    // 先预分配完整体积的临时文件，各分段按偏移直接写入，避免二次合并。
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(temp_path)
        .and_then(|file| file.set_len(total))
        .map_err(|error| DownloadError::new(format!("创建临时文件失败: {error}")))?;

    let mut results = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for segment in 0..segment_count {
            let start = segment * segment_size;
            let end = if segment + 1 == segment_count {
                total - 1
            } else {
                start + segment_size - 1
            };
            let progress = Arc::clone(&progress);
            let abort = Arc::clone(&abort);
            let cancel = Arc::clone(cancel);
            let tx = tx.clone();
            let temp_path = temp_path.to_path_buf();
            handles.push(scope.spawn(move || {
                download_segment(
                    client, url, &temp_path, segment, start, end, index, total, &progress, &cancel,
                    &abort, &tx,
                )
                .inspect_err(|_| abort.store(true, Ordering::Relaxed))
            }));
        }
        for handle in handles {
            results.push(match handle.join() {
                Ok(result) => result,
                Err(_) => Err(DownloadError::new("下载线程异常退出")),
            });
        }
    });

    // 任一分片失败会中断其余分片；优先上报超时，避免被“已中断”掩盖。
    let mut errors: Vec<DownloadError> = results.into_iter().filter_map(Result::err).collect();
    if let Some(index) = errors.iter().position(|error| error.timed_out) {
        return Err(errors.swap_remove(index));
    }
    if !errors.is_empty() {
        return Err(errors.swap_remove(0));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn download_segment(
    client: &reqwest::blocking::Client,
    url: &str,
    temp_path: &Path,
    segment: u64,
    start: u64,
    end: u64,
    index: usize,
    total: u64,
    progress: &Arc<Mutex<Vec<u64>>>,
    cancel: &Arc<AtomicBool>,
    abort: &Arc<AtomicBool>,
    tx: &Sender<DownloadEvent>,
) -> Result<u64, DownloadError> {
    let expected = end - start + 1;
    let mut response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, referer_for(url))
        .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
        .send()
        .map_err(|error| DownloadError::from_reqwest("下载请求失败", error))?
        .error_for_status()
        .map_err(|error| DownloadError::from_reqwest("服务器返回错误", error))?;
    if response.status().as_u16() != 206 {
        return Err(DownloadError::new("服务器不支持分段下载"));
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(temp_path)
        .map_err(|error| DownloadError::new(format!("打开临时文件失败: {error}")))?;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| DownloadError::new(format!("定位写入位置失败: {error}")))?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut downloaded = 0u64;
    let mut last_reported = 0u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::new("下载已取消"));
        }
        if abort.load(Ordering::Relaxed) {
            return Err(DownloadError::new("下载已中断"));
        }
        let read = response
            .read(&mut buffer)
            .map_err(|error| DownloadError::from_io("读取数据失败", error))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| DownloadError::new(format!("写入分片失败: {error}")))?;
        downloaded += read as u64;
        if downloaded - last_reported >= 128 * 1024 {
            last_reported = downloaded;
            report_segment_progress(progress, segment, downloaded, index, total, tx);
        }
    }

    if downloaded != expected {
        return Err(DownloadError::new(format!(
            "分片长度错误: 期望 {expected} 字节，实际 {downloaded} 字节"
        )));
    }
    report_segment_progress(progress, segment, downloaded, index, total, tx);
    Ok(downloaded)
}

fn report_segment_progress(
    progress: &Arc<Mutex<Vec<u64>>>,
    segment: u64,
    downloaded: u64,
    index: usize,
    total: u64,
    tx: &Sender<DownloadEvent>,
) {
    let Ok(mut progress) = progress.lock() else {
        return;
    };
    let Some(slot) = progress.get_mut(segment as usize) else {
        return;
    };
    *slot = downloaded;
    let total_downloaded = progress.iter().sum();
    let _ = tx.send(DownloadEvent::Progress {
        index,
        downloaded: total_downloaded,
        total,
    });
}

pub(crate) fn referer_for(url: &str) -> &'static str {
    if url.contains("xhscdn.com") || url.contains("xiaohongshu.com") {
        "https://www.xiaohongshu.com/"
    } else {
        "https://www.douyin.com/"
    }
}

pub fn clear_cache(cache_dir: &Path) -> Result<(), String> {
    if !cache_dir.exists() {
        return Ok(());
    }
    if !cache_dir.is_dir() {
        return std::fs::remove_file(cache_dir).map_err(|error| error.to_string());
    }
    let entries = std::fs::read_dir(cache_dir).map_err(|error| error.to_string())?;
    let mut errors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let result = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", path.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_file_name_keeps_mp4_suffix() {
        let directory = std::env::temp_dir().join("reels_no_such_download_dir");
        assert_eq!(
            download_file_name(&directory, "测试标题", "mp4"),
            "测试标题.mp4"
        );
    }

    #[test]
    fn stalled_response_is_reported_as_timeout() {
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/stall.jpg", listener.local_addr().expect("addr"));
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            // 收到请求后不返回任何响应，直到测试确认超时。
            let _ = release_rx.recv();
            drop(stream);
        });

        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_millis(200))
            .timeout(Duration::from_millis(200))
            .build()
            .expect("client");
        let directory = std::env::temp_dir().join("reels_stall_test");
        std::fs::create_dir_all(&directory).expect("create dir");
        let temp_path = directory.join("stall.jpg.part");
        let (tx, _rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let error = download_one(&client, &url, &temp_path, "jpg", 0, &cancel, &tx)
            .expect_err("stalled response should fail");
        assert!(error.timed_out, "expected timeout, got {error:?}");

        let _ = release_tx.send(());
        server.join().expect("server thread");
        let _ = std::fs::remove_file(&temp_path);
        let _ = std::fs::remove_dir(&directory);
    }

    #[test]
    fn parallel_download_writes_segments_in_place() {
        use std::net::TcpListener;

        let total: u64 = 1024;
        let body: Vec<u8> = (0..total as usize).map(|index| index as u8).collect();
        let server_body = body.clone();

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/range.bin", listener.local_addr().expect("addr"));
        let server = std::thread::spawn(move || {
            let mut served = 0;
            while served < PARALLEL_SEGMENT_COUNT as usize {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut request = Vec::new();
                let mut buffer = [0u8; 1024];
                loop {
                    let read = stream.read(&mut buffer).expect("read request");
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&request).to_string();
                let range = request
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("range")
                            .then(|| value.trim().strip_prefix("bytes=").map(str::to_string))
                            .flatten()
                    })
                    .expect("range header");
                let (start, end) = {
                    let (start, end) = range.split_once('-').expect("range values");
                    (
                        start.parse::<u64>().expect("range start"),
                        end.parse::<u64>().expect("range end"),
                    )
                };
                let slice = &server_body[start as usize..=(end as usize)];
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    slice.len()
                );
                stream.write_all(head.as_bytes()).expect("write head");
                stream.write_all(slice).expect("write body");
                served += 1;
            }
        });

        let client = reqwest::blocking::Client::new();
        let directory = std::env::temp_dir().join("reels_parallel_test");
        std::fs::create_dir_all(&directory).expect("create dir");
        let temp_path = directory.join("out.bin");
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        download_parallel(&client, &url, &temp_path, total, 0, &cancel, &tx)
            .expect("parallel download");

        let downloaded = std::fs::read(&temp_path).expect("read result");
        assert_eq!(downloaded, body);
        assert!(matches!(rx.recv(), Ok(DownloadEvent::Started { .. })));
        assert!(matches!(rx.recv(), Ok(DownloadEvent::Progress { .. })));
        server.join().expect("server thread");
        let _ = std::fs::remove_file(&temp_path);
        let _ = std::fs::remove_dir(&directory);
    }
}
