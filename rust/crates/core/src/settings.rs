//! 本地设置模型（JSON 文件持久化）。

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// `system` | `light` | `dark`
    pub theme_mode: String,
    pub last_directory: String,
    pub skip_location_prompt: bool,
    pub remember_location: bool,
    pub save_to_gallery: bool,
    /// 小红书请求 Cookie（可选，默认留白）。
    pub xhs_cookie: String,
    /// 实况图导出：iOS 封装为 Live Photo，Android / Windows 封装为 Google Motion Photo；
    /// false 则保存为照片 + 视频。
    pub live_photo_export: bool,
    /// 解析结果页展示版本：`geek` | `simple`。
    pub result_view: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_mode: "system".to_string(),
            last_directory: String::new(),
            skip_location_prompt: false,
            remember_location: false,
            save_to_gallery: true,
            xhs_cookie: String::new(),
            live_photo_export: true,
            result_view: "geek".to_string(),
        }
    }
}

impl AppSettings {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        fs::write(path, text)
    }

    pub fn is_dark(&self, system_dark: bool) -> bool {
        match self.theme_mode.as_str() {
            "dark" => true,
            "light" => false,
            _ => system_dark,
        }
    }
}
