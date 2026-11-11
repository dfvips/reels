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
            let config_dir = home.join(".config").join("douyin_parser");
            let cache_dir = home.join("Library").join("Caches").join("douyin_parser");
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
                .join("com.dreamfly.douyinParser");
            #[cfg(target_os = "windows")]
            let config_dir = std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.clone())
                .join("com.dreamfly")
                .join("douyinParser");
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let config_dir = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
                .join("douyin_parser");

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
            total += meta.len();
        }
    }
    total
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
        format!("{value:.1} {}", UNITS[unit])
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
