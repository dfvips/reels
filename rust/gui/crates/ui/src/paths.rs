//! 应用目录（配置 / 缓存 / 默认下载位置）。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PATH_OVERRIDE: OnceLock<AppPaths> = OnceLock::new();

/// 移动端宿主在启动时注入应用沙盒目录。
pub fn set_override(paths: AppPaths) {
    let _ = PATH_OVERRIDE.set(paths);
}

pub fn resolve() -> AppPaths {
    PATH_OVERRIDE
        .get()
        .cloned()
        .unwrap_or_else(AppPaths::discover)
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub default_download_dir: PathBuf,
    pub mobile: bool,
}

impl AppPaths {
    pub fn discover() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let config_dir = home.join(".config").join("reels");
            let cache_dir = home.join("Library").join("Caches").join("reels");
            let downloads = home.join("Documents");
            Self {
                config_dir,
                cache_dir,
                default_download_dir: downloads,
                mobile: true,
            }
        } else {
            let home = home_dir();
            #[cfg(target_os = "macos")]
            let config_dir = home
                .join("Library")
                .join("Application Support")
                .join("com.dreamfly.reels");
            #[cfg(target_os = "windows")]
            let config_dir = std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.clone())
                .join("com.dreamfly")
                .join("reels");
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let config_dir = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("reels");

            let cache_dir = config_dir.join("cache");
            let default_download_dir = home.join("Downloads");
            Self {
                config_dir,
                cache_dir,
                default_download_dir,
                mobile: false,
            }
        }
    }

    pub fn with_roots(config_dir: PathBuf, cache_dir: PathBuf, downloads: PathBuf) -> Self {
        Self {
            config_dir,
            cache_dir,
            default_download_dir: downloads,
            mobile: cfg!(any(target_os = "android", target_os = "ios")),
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// 应用改名前（douyin_parser 时代）遗留的缓存目录，清理缓存时一并统计和删除。
    pub fn legacy_cache_dirs(&self) -> Vec<PathBuf> {
        let home = home_dir();
        #[cfg(target_os = "ios")]
        {
            let mut paths = Vec::new();
            for root in [
                home.join("Library").join("Caches"),
                home.join("Library").join("Application Support"),
            ] {
                for name in [
                    "douyin_parser",
                    "douyinParser",
                    "douyin-parser",
                    "com.dreamfly.douyinParser",
                    "com.dreamfly.douyin-parser",
                ] {
                    paths.push(root.join(name));
                }
                if let Ok(entries) = std::fs::read_dir(&root) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                        if name.contains("douyin") || name.contains("douyinparser") {
                            paths.push(entry.path());
                        }
                    }
                }
            }
            paths.push(home.join("Documents").join("douyin_parser"));
            paths.sort();
            paths.dedup();
            paths
        }
        #[cfg(target_os = "android")]
        {
            Vec::new()
        }
        #[cfg(target_os = "macos")]
        {
            vec![home
                .join("Library")
                .join("Application Support")
                .join("com.dreamfly.douyinParser")
                .join("cache")]
        }
        #[cfg(target_os = "windows")]
        {
            vec![std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.clone())
                .join("com.dreamfly")
                .join("douyinParser")
                .join("cache")]
        }
        #[cfg(not(any(
            target_os = "ios",
            target_os = "android",
            target_os = "macos",
            target_os = "windows"
        )))]
        {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"));
            vec![config.join("douyin_parser").join("cache")]
        }
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

pub fn is_desktop() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ))
}

pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

pub fn is_windows() -> bool {
    cfg!(target_os = "windows")
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            total += dir_size(&entry.path());
        } else {
            // 与系统存储统计口径一致：按实际占用的磁盘块计算，
            // 预分配的分片文件未写入的部分（稀疏空洞）不计入。
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                total += meta.blocks() * 512;
            }
            #[cfg(not(unix))]
            {
                total += meta.len();
            }
        }
    }
    total
}

/// 单个文件实际占用的磁盘字节数（与系统存储统计口径一致）。
pub fn allocated_file_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::metadata(path) else {
        return 0;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

/// iOS 旧版本把下载成品直接留在沙盒 `Documents` 里，这些文件不在缓存目录
/// 中，清理缓存时扫不到却一直占用系统“文稿与数据”空间。只挑下载产物的
/// 后缀，避免误删用户自己放进去的其他文件。
pub fn leftover_download_files() -> Vec<PathBuf> {
    #[cfg(target_os = "ios")]
    {
        let home = home_dir();
        let mut files = Vec::new();
        for root in [
            home.join("Documents"),
            home.join("tmp"),
            home.join("Library").join("Application Support"),
        ] {
            collect_download_artifacts(&root, &mut files);
        }
        files.sort();
        files.dedup();
        files
    }
    #[cfg(not(target_os = "ios"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "ios")]
fn collect_download_artifacts(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_download_artifacts(&path, files);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        if extension.as_deref().is_some_and(|extension| {
            matches!(
                extension,
                "mp4"
                    | "mov"
                    | "m4v"
                    | "jpg"
                    | "jpeg"
                    | "png"
                    | "webp"
                    | "heic"
                    | "heif"
                    | "gif"
                    | "part"
                    | "download"
                    | "tmp"
            )
        }) {
            files.push(path);
        }
    }
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        let value = format!("{value:.1}");
        format!(
            "{} {}",
            value.strip_suffix(".0").unwrap_or(&value),
            UNITS[unit]
        )
    }
}

pub fn ensure_dir(path: &Path) {
    let _ = std::fs::create_dir_all(path);
}

pub fn open_directory(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn dir_size_reports_allocated_blocks_for_sparse_files() {
        let directory = std::env::temp_dir().join("reels_sparse_test");
        let _ = std::fs::create_dir_all(&directory);
        let file = directory.join("sparse.part");
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&file)
            .and_then(|file| file.set_len(10 * 1024 * 1024))
            .expect("pre-allocate sparse file");

        let size = dir_size(&directory);
        assert!(size < 1024 * 1024, "sparse file counted as {size} bytes");

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn format_size_keeps_zero_and_trims_fractional_zero() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(63 * 1024 * 1024), "63 MB");
        assert_eq!(format_size(539 * 1024 * 1024 + 512 * 1024), "539.5 MB");
    }
}
