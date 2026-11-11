//! 抖音视频解析核心逻辑。
//!
//! 模块划分：
//! - [`sm3`] / [`a_bogus`]：网页接口签名
//! - [`url_parser`]：链接提取与视频 ID 解析
//! - [`api`]：签名接口（aweme detail）
//! - [`http`]：HTML / 重定向 / 播放地址
//! - [`extractor`]：解析编排（接口优先，HTML 回退）
//! - [`hellotik`]：hellotik.app RAW 解析协议（xhs_app 同款）
//! - [`files`]：跨平台安全文件名
//! - [`script`]：aria2c / downloader / RobustDownloader 脚本导出
//! - [`settings`]：本地设置模型
//! - [`xhs`]：小红书笔记解析
//! - [`mp4`]：Range 读取 moov/tkhd 识别 MP4 分辨率

pub mod a_bogus;
pub mod api;
pub mod extractor;
pub mod files;
pub mod hellotik;
pub mod http;
pub mod mp4;
pub mod script;
pub mod settings;
pub mod sm3;
pub mod url_parser;
pub mod xhs;
