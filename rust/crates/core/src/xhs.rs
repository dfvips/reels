//! 小红书笔记解析（与 xhs_app 的 XhsWebviewParser 同协议）。
//!
//! xhs_app 用 HeadlessInAppWebView 加载笔记页后注入 JS，从
//! `window.__INITIAL_STATE__` 中定位笔记节点（walk 深度 4，
//! 命中条件 `noteId && (imageList || video)`），再从
//! `video.consumer.originVideoKey`、`video.media.stream` 与
//! `imageList` 构造结果。本模块用纯 HTTP 请求同页面并复刻
//! 相同的字段处理。

use std::sync::LazyLock;

use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, REFERER, USER_AGENT};
use serde_json::Value;

use crate::http::HttpClient;

/// xhs_app 的 HeadlessInAppWebView 所用的移动端 UA。
const WEBVIEW_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.6 Mobile/15E148 Safari/604.1";
const XHS_ORIGIN: &str = "https://www.xiaohongshu.com/";
const VIDEO_CDN: &str = "https://sns-video-bd.xhscdn.com/";
const IMAGE_CDN: &str = "https://sns-img-hw.xhscdn.com/";
const IMAGE_NOTE_CDN: &str = "https://ci.xiaohongshu.com/";

static XHS_HOST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^https?://[\w.-]*xiaohongshu\.com/|^https?://xhslink\.(?:com|cn)/").unwrap()
});

static INITIAL_STATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<script>window\.__INITIAL_STATE__=(.*?)</script>").unwrap());

static PROFILE_NOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(https?://[\w.-]*xiaohongshu\.com/)user/profile/[^/?#]+/([^/?#]+)(\?.*)?$")
        .unwrap()
});

#[derive(Debug, Clone, Default)]
pub struct XhsVideoInfo {
    pub url: String,
    pub size: Option<u64>,
    pub duration: Option<u64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub quality_type: Option<String>,
    pub hdr_type: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct XhsImageInfo {
    pub url: String,
    pub high_quality_url: String,
    pub raw_url: String,
    pub size_bytes: Option<u64>,
    /// 按 CDN 响应的 Content-Type 得到的实际扩展名（heic/jpg/png/...）。
    pub extension: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub live_photo: bool,
    /// live photo 附带的短视频流。
    pub live_videos: Vec<XhsVideoInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct XhsNote {
    pub note_id: String,
    pub title: String,
    pub desc: String,
    pub note_type: String,
    pub videos: Vec<XhsVideoInfo>,
    pub images: Vec<XhsImageInfo>,
}

pub fn is_xhs_url(url: &str) -> bool {
    XHS_HOST.is_match(url)
}

pub fn request_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(WEBVIEW_USER_AGENT));
    headers.insert(REFERER, HeaderValue::from_static(XHS_ORIGIN));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers
}

/// 请求并解析一条小红书笔记。
///
/// 与 xhs_app 一致，先访问首页建立会话 Cookie，再请求笔记页。
pub fn parse_note(
    client: &HttpClient,
    url: &str,
    user_cookie: Option<&str>,
) -> Result<XhsNote, String> {
    let note_url = resolve_note_url(client, url)?;
    let headers = request_headers();
    let cookie = merge_cookies(
        client
            .fetch_html_ex(XHS_ORIGIN, &headers, user_cookie)
            .map(|(_, cookie)| cookie)
            .unwrap_or_default(),
        user_cookie,
    );
    let html = if cookie.is_empty() {
        client.fetch_html_with(&note_url, &headers)?
    } else {
        client.fetch_html_ex(&note_url, &headers, Some(&cookie))?.0
    };
    parse_note_html(&html, client)
        .ok_or_else(|| "未能获取小红书笔记数据，链接可能已失效或需要登录".to_string())
}

fn merge_cookies(session: String, user: Option<&str>) -> String {
    let Some(user) = user.map(str::trim).filter(|value| !value.is_empty()) else {
        return session;
    };
    let user_pairs: Vec<&str> = user
        .split(';')
        .map(str::trim)
        .filter(|pair| pair.contains('='))
        .collect();
    let mut merged: Vec<String> = Vec::new();
    let user_name = |pair: &str| pair.split('=').next().unwrap_or("").trim().to_ascii_lowercase();
    for existing in session.split(';') {
        let existing = existing.trim();
        if existing.is_empty() || !existing.contains('=') {
            continue;
        }
        let name = existing.split('=').next().unwrap_or("").trim().to_ascii_lowercase();
        if user_pairs.iter().any(|pair| user_name(pair) == name) {
            continue;
        }
        merged.push(existing.to_string());
    }
    for pair in user_pairs {
        merged.push(pair.to_string());
    }
    merged.join("; ")
}

/// 展开短链并处理 website-login 重定向（对齐 _resolveNoteUrl）。
fn resolve_note_url(client: &HttpClient, url: &str) -> Result<String, String> {
    let mut current = url.to_string();
    if current.contains("xhslink.") {
        for _ in 0..5 {
            let Some(redirected) = client.get_redirect_url(&current) else {
                break;
            };
            current = redirected;
        }
    }
    if current.contains("website-login") {
        if let Some(redirect_path) = redirect_path_of(&current) {
            return Ok(redirect_path);
        }
    }
    if let Some(normalized) = normalize_profile_url(&current) {
        current = normalized;
    }
    Ok(current)
}

/// `user/profile/{uid}/{note_id}` 与 `explore/{note_id}` 等价，
/// 统一归一化为 explore 页并保留 xsec_token 等查询参数。
fn normalize_profile_url(url: &str) -> Option<String> {
    let caps = PROFILE_NOTE.captures(url)?;
    let note_id = &caps[2];
    let query = caps.get(3).map(|m| m.as_str()).unwrap_or("");
    Some(format!("{}explore/{note_id}{query}", &caps[1]))
}

fn redirect_path_of(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == "redirectPath" {
                let decoded = percent_decode(value).replace_first("http://", "https://");
                return Some(decoded);
            }
        }
    }
    None
}

trait ReplaceFirst {
    fn replace_first(&self, from: &str, to: &str) -> String;
}

impl ReplaceFirst for String {
    fn replace_first(&self, from: &str, to: &str) -> String {
        match self.find(from) {
            Some(index) => {
                let mut result = String::with_capacity(self.len());
                result.push_str(&self[..index]);
                result.push_str(to);
                result.push_str(&self[index + from.len()..]);
                result
            }
            None => self.clone(),
        }
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 从笔记页 HTML 中解析笔记信息。
pub fn parse_note_html(html: &str, client: &HttpClient) -> Option<XhsNote> {
    let raw = INITIAL_STATE.captures(html)?.get(1)?.as_str();
    // INITIAL_STATE 是 JavaScript 对象字面量，可能包含 `undefined`，
    // xhs_app 经 WebView 的 JSON.stringify 序列化时这些字段会被丢弃；
    // 这里直接替换为 null 以获得合法 JSON。
    let json = raw.replace("undefined", "null");
    let value: Value = serde_json::from_str(&json).ok()?;
    let note = find_note(&value, 0)?;
    Some(build_note(&note, client))
}

/// 复刻 xhs_app 注入的 JS：深度 4 以内寻找
/// `node.noteId && (node.imageList || node.video)` 的节点。
fn find_note(value: &Value, depth: usize) -> Option<&Value> {
    if depth > 4 {
        return None;
    }
    match value {
        Value::Object(map) => {
            let has_media = map.contains_key("imageList") || map.contains_key("video");
            if map.contains_key("noteId") && has_media {
                return Some(value);
            }
            for child in map.values() {
                if let Some(found) = find_note(child, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => {
            for child in items {
                if let Some(found) = find_note(child, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn build_note(note: &Value, client: &HttpClient) -> XhsNote {
    let string_field = |key: &str| {
        note.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let mut result = XhsNote {
        note_id: string_field("noteId"),
        title: string_field("title"),
        desc: string_field("desc"),
        note_type: string_field("type"),
        ..Default::default()
    };
    if result.note_type == "video" {
        result.videos = extract_videos(note, client);
    }
    result.images = extract_images(note.get("imageList"), client);
    result
}

/// 复刻 ParserService::extractVideos：
/// RAW 来自 `video.consumer.originVideoKey`，HD 来自
/// `video.media.stream` 的 h264/h265/h266/av1（masterUrl，http 升级 https）。
fn extract_videos(note: &Value, client: &HttpClient) -> Vec<XhsVideoInfo> {
    let Some(video) = note.get("video") else {
        return Vec::new();
    };
    let mut videos = Vec::new();

    let duration = video
        .get("capa")
        .and_then(|capa| capa.get("duration"))
        .and_then(Value::as_u64);
    if let Some(key) = video
        .get("consumer")
        .and_then(|consumer| consumer.get("originVideoKey"))
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
    {
        let url = format!("{VIDEO_CDN}{key}");
        let size = client.head_content_length(&url);
        // RAW 条目无 width/height，通过 Range 读取 moov/tkhd 识别分辨率。
        let dimensions = crate::mp4::probe_dimensions(client, &url);
        videos.push(XhsVideoInfo {
            url,
            size,
            duration,
            quality_type: Some("RAW".to_string()),
            width: dimensions.map(|dimensions| u64::from(dimensions.width)),
            height: dimensions.map(|dimensions| u64::from(dimensions.height)),
            ..Default::default()
        });
    }

    let Some(streams) = video.get("media").and_then(|media| media.get("stream")) else {
        return videos;
    };
    videos.extend(extract_stream_videos(streams));
    videos
}

/// 解析 `stream` 对象（视频笔记的 `video.media.stream` 与
/// 图文笔记 live photo 的 `imageList[].stream` 结构一致）。
fn extract_stream_videos(streams: &Value) -> Vec<XhsVideoInfo> {
    let Some(streams) = streams.as_object() else {
        return Vec::new();
    };
    let mut videos = Vec::new();
    for codec in ["h264", "h265", "h266", "av1"] {
        let Some(items) = streams.get(codec).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(mut url) = item
                .get("masterUrl")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    item.get("backupUrls")
                        .and_then(Value::as_array)
                        .and_then(|urls| urls.first())
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .filter(|url| !url.is_empty())
            else {
                continue;
            };
            if url.starts_with("http://") {
                url = url.replacen("http://", "https://", 1);
            }
            videos.push(XhsVideoInfo {
                url,
                size: item.get("size").and_then(Value::as_u64),
                duration: item
                    .get("videoDuration")
                    .and_then(Value::as_u64)
                    .map(|ms| ms / 1000),
                width: item.get("width").and_then(Value::as_u64),
                height: item.get("height").and_then(Value::as_u64),
                video_codec: item
                    .get("videoCodec")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                audio_codec: item
                    .get("audioCodec")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                quality_type: item
                    .get("qualityType")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                hdr_type: item.get("hdrType").and_then(Value::as_i64),
            });
        }
    }
    videos
}

/// 复刻 ParserService::extractImages：
/// key 取地址 `!` 前的最后一段，路径带 note 时用 ci.xiaohongshu.com，
/// 否则用 sns-img-hw.xhscdn.com，高质量地址追加
/// `?imageView2/format/raw`。
fn extract_images(image_list: Option<&Value>, client: &HttpClient) -> Vec<XhsImageInfo> {
    let Some(items) = image_list.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut images = Vec::new();
    for item in items {
        let Some(source) = item
            .get("urlDefault")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty())
            .or_else(|| item.get("url").and_then(Value::as_str))
            .filter(|url| !url.is_empty())
        else {
            continue;
        };
        let before_suffix = source.split('!').next().unwrap_or(source);
        let segments: Vec<&str> = before_suffix.split('/').filter(|s| !s.is_empty()).collect();
        // 与 xhs_app 一致：note 相关目录（如 notes_pre_post）保留目录
        // 并使用 ci.xiaohongshu.com，其余取最后一段使用 sns-img-hw。
        let note_start = segments
            .iter()
            .position(|segment| segment.contains("note"));
        let (base, key) = match note_start {
            Some(index) => (
                IMAGE_NOTE_CDN,
                segments[index..].join("/"),
            ),
            None => match segments.last() {
                Some(key) => (IMAGE_CDN, (*key).to_string()),
                None => continue,
            },
        };
        let raw_url = format!("{base}{key}");
        let (size_bytes, content_type) = client.head_meta(&raw_url);
        images.push(XhsImageInfo {
            url: source.to_string(),
            high_quality_url: format!("{base}{key}?imageView2/format/raw"),
            raw_url,
            size_bytes,
            extension: image_extension(content_type.as_deref()),
            width: item.get("width").and_then(Value::as_u64),
            height: item.get("height").and_then(Value::as_u64),
            live_photo: item
                .get("livePhoto")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            live_videos: item
                .get("stream")
                .map(extract_stream_videos)
                .unwrap_or_default(),
        });
    }
    images
}

fn image_extension(content_type: Option<&str>) -> Option<String> {
    let mime = content_type?.split(';').next()?.trim().to_ascii_lowercase();
    let extension = match mime.as_str() {
        "image/heic" | "image/heif" => "heic",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/avif" => "avif",
        "image/gif" => "gif",
        _ => return None,
    };
    Some(extension.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_note_within_walk_depth() {
        let html = r#"<script>window.__INITIAL_STATE__={"noteData":{"data":{"noteData":{"noteId":"abc","title":"t","desc":"d","type":"video","video":{"capa":{"duration":10},"consumer":{"originVideoKey":"ros-cn-sh/pre_post/key1"},"media":{"stream":{"h264":[{"masterUrl":"http://sns-video-v2.xhscdn.com/stream/x_259.mp4","size":1,"videoDuration":10000,"width":720,"height":1280,"videoCodec":"h264","audioCodec":"aac","qualityType":"HD","hdrType":0}]}}},"imageList":[{"url":"http://sns-webpic-qc.xhscdn.com/a/b/1040g008key!h5_1080jpg","width":100,"height":200,"livePhoto":false}]}}}}</script>"#;
        let client = HttpClient::new().unwrap();
        let note = parse_note_html(html, &client).unwrap();
        assert_eq!(note.note_id, "abc");
        assert_eq!(note.title, "t");
        assert_eq!(note.note_type, "video");
        let videos = note.videos;
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].quality_type.as_deref(), Some("RAW"));
        assert_eq!(
            videos[0].url,
            "https://sns-video-bd.xhscdn.com/ros-cn-sh/pre_post/key1"
        );
        assert_eq!(videos[0].duration, Some(10));
        assert_eq!(videos[1].quality_type.as_deref(), Some("HD"));
        assert_eq!(
            videos[1].url,
            "https://sns-video-v2.xhscdn.com/stream/x_259.mp4"
        );
        assert_eq!(videos[1].duration, Some(10));
        assert_eq!(videos[1].width, Some(720));
        let images = note.images;
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].raw_url,
            "https://sns-img-hw.xhscdn.com/1040g008key"
        );
        assert_eq!(
            images[0].high_quality_url,
            "https://sns-img-hw.xhscdn.com/1040g008key?imageView2/format/raw"
        );
    }

    #[test]
    fn image_note_path_uses_ci_domain() {
        let note = json!({
            "noteId": "x",
            "type": "normal",
            "imageList": [
                {
                    "url": "http://sns-webpic-qc.xhscdn.com/2026/a/b/notes_pre_post/1040g3k8key!h5_1080jpg",
                    "width": 1080,
                    "height": 1441,
                    "livePhoto": true
                }
            ]
        });
        let client = HttpClient::new().unwrap();
        let images = extract_images(note.get("imageList"), &client);
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].raw_url,
            "https://ci.xiaohongshu.com/notes_pre_post/1040g3k8key"
        );
        assert!(images[0].live_photo);
    }

    #[test]
    fn video_note_without_stream_keeps_raw_only() {
        let note = json!({
            "noteId": "y",
            "type": "video",
            "video": {
                "capa": {"duration": 25},
                "consumer": {"originVideoKey": "spectrum/key2"}
            }
        });
        let client = HttpClient::new().unwrap();
        let videos = extract_videos(&note, &client);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].url, "https://sns-video-bd.xhscdn.com/spectrum/key2");
        assert_eq!(videos[0].duration, Some(25));
    }

    #[test]
    fn is_xhs_url_matches_hosts() {
        assert!(is_xhs_url(
            "https://www.xiaohongshu.com/discovery/item/x?xsec_token=1"
        ));
        assert!(is_xhs_url("https://xhslink.com/abc"));
        assert!(!is_xhs_url("https://www.douyin.com/video/1"));
    }

    #[test]
    fn profile_url_normalizes_to_explore() {
        assert_eq!(
            normalize_profile_url(
                "https://www.xiaohongshu.com/user/profile/65c5fd5600000000110173a5/6773508e0000000009015fa0?xsec_token=ABQTBZnyEMUTk9WVeN-qHEDWqSaJGte5XN0mFSwJEWGXo=&xsec_source=pc_user",
            ),
            Some(
                "https://www.xiaohongshu.com/explore/6773508e0000000009015fa0?xsec_token=ABQTBZnyEMUTk9WVeN-qHEDWqSaJGte5XN0mFSwJEWGXo=&xsec_source=pc_user".to_string(),
            )
        );
        assert_eq!(
            normalize_profile_url("https://www.xiaohongshu.com/user/profile/abc/def"),
            Some("https://www.xiaohongshu.com/explore/def".to_string())
        );
        assert_eq!(
            normalize_profile_url("https://www.xiaohongshu.com/explore/6773508e0000000009015fa0?xsec_token=1"),
            None
        );
    }
}
