//! 视频下载（后台线程 + 进度事件）。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use douyin_core::files::{safe_file_name, unique_file_name};

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
    },
    Finished,
}

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub fn start_downloads(
    urls: Vec<String>,
    titles: Vec<String>,
    extensions: Vec<Option<String>>,
    directory: PathBuf,
    cache_dir: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: Sender<DownloadEvent>,
) {
    std::thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                let _ = tx.send(DownloadEvent::Failed {
                    index: 0,
                    error: format!("初始化下载失败: {error}"),
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

            match download_one(&client, url, &temp_path, index, &cancel, &tx) {
                Ok(()) => {
                    let _ = std::fs::create_dir_all(&directory);
                    match std::fs::rename(&temp_path, &destination) {
                        Ok(()) => {
                            let _ = tx.send(DownloadEvent::Completed {
                                index,
                                path: destination,
                            });
                        }
                        Err(rename_error) => match std::fs::copy(&temp_path, &destination) {
                            Ok(_) => {
                                let _ = std::fs::remove_file(&temp_path);
                                let _ = tx.send(DownloadEvent::Completed {
                                    index,
                                    path: destination,
                                });
                            }
                            Err(copy_error) => {
                                let _ = std::fs::remove_file(&temp_path);
                                let _ = tx.send(DownloadEvent::Failed {
                                    index,
                                    error: format!(
                                        "保存文件失败: 移动失败: {rename_error}; 复制失败: {copy_error}"
                                    ),
                                });
                            }
                        },
                    }
                }
                Err(error) => {
                    let _ = std::fs::remove_file(&temp_path);
                    let _ = tx.send(DownloadEvent::Failed { index, error });
                }
            }
        }

        let _ = tx.send(DownloadEvent::Finished);
    });
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
    index: usize,
    cancel: &Arc<AtomicBool>,
    tx: &Sender<DownloadEvent>,
) -> Result<(), String> {
    let mut response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::REFERER, referer_for(url))
        .send()
        .map_err(|error| format!("下载请求失败: {error}"))?
        .error_for_status()
        .map_err(|error| format!("服务器返回错误: {error}"))?;

    let total = response.content_length().unwrap_or(0);
    let _ = tx.send(DownloadEvent::Started { index, total });

    let mut file =
        std::fs::File::create(temp_path).map_err(|error| format!("创建临时文件失败: {error}"))?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut downloaded = 0u64;
    let mut last_reported = 0u64;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("下载已取消".to_string());
        }
        let read = response
            .read(&mut buffer)
            .map_err(|error| format!("读取数据失败: {error}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| format!("写入文件失败: {error}"))?;
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

fn referer_for(url: &str) -> &'static str {
    if url.contains("xhscdn.com") || url.contains("xiaohongshu.com") {
        "https://www.xiaohongshu.com/"
    } else {
        "https://www.douyin.com/"
    }
}

pub fn clear_cache(cache_dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(cache_dir).map_err(|error| error.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
        } else {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloaded_file_name_keeps_mp4_suffix() {
        let directory = std::env::temp_dir().join("douyin_parser_no_such_download_dir");
        assert_eq!(
            download_file_name(&directory, "测试标题", "mp4"),
            "测试标题.mp4"
        );
    }
}
