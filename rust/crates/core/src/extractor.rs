//! 解析编排：链接解析、接口请求与播放地址提取。

use crate::api::DouyinApi;
use crate::http::HttpClient;
use crate::url_parser;
use crate::xhs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPlatform {
    Douyin,
    Xhs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Image,
}

/// 一条可下载媒体：一个结果卡片内的一行链接。
#[derive(Debug, Clone, Default)]
pub struct MediaEntry {
    pub url: String,
    /// 下载扩展名；None 表示按默认 mp4 处理。
    pub extension: Option<String>,
    pub duration_secs: Option<u64>,
    pub size_bytes: Option<u64>,
    /// 清晰度标签（xhs: RAW/HD；抖音: 1080P 等）。
    pub quality: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
}

/// 一个解析结果卡片：同标题下的一组媒体链接。
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub success: bool,
    pub original_url: String,
    pub platform: MediaPlatform,
    pub media_kind: MediaKind,
    pub title: Option<String>,
    pub entries: Vec<MediaEntry>,
    pub error: Option<String>,
}

impl ExtractionResult {
    fn failure(original_url: &str, platform: MediaPlatform, error: impl Into<String>) -> Self {
        Self {
            success: false,
            original_url: original_url.to_string(),
            platform,
            media_kind: MediaKind::Video,
            title: None,
            entries: Vec::new(),
            error: Some(error.into()),
        }
    }

    fn success(
        original_url: &str,
        platform: MediaPlatform,
        media_kind: MediaKind,
        title: Option<String>,
        entries: Vec<MediaEntry>,
    ) -> Self {
        Self {
            success: true,
            original_url: original_url.to_string(),
            platform,
            media_kind,
            title,
            entries,
            error: None,
        }
    }
}

pub struct VideoExtractor {
    http: HttpClient,
    api: DouyinApi,
    xhs_cookie: Option<String>,
}

impl VideoExtractor {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: HttpClient::new()?,
            api: DouyinApi::new()?,
            xhs_cookie: None,
        })
    }

    /// 小红书专用 Cookie（可选，默认留白）。
    pub fn with_xhs_cookie(mut self, cookie: Option<String>) -> Self {
        self.xhs_cookie = cookie.filter(|cookie| !cookie.trim().is_empty());
        self
    }

    pub fn extract_multiple(&self, input: &str) -> Vec<ExtractionResult> {
        url_parser::extract_urls(input)
            .iter()
            .flat_map(|url| self.extract_one(url))
            .collect()
    }

    fn extract_one(&self, url: &str) -> Vec<ExtractionResult> {
        if xhs::is_xhs_url(url) {
            return vec![self.extract_xhs(url)];
        }
        vec![self.extract_douyin(url)]
    }

    fn extract_xhs(&self, url: &str) -> ExtractionResult {
        match xhs::parse_note(&self.http, url, self.xhs_cookie.as_deref()) {
            Ok(note) => {
                let title = if note.title.is_empty() {
                    note.desc.clone()
                } else {
                    note.title.clone()
                };
                if !note.videos.is_empty() {
                    // 与 xhs_app 一致：RAW 原画优先，其余按 stream 顺序（h264 在前）。
                    let mut videos = note.videos.clone();
                    videos.sort_by_key(|video| {
                        if video.quality_type.as_deref() == Some("RAW") {
                            0
                        } else {
                            1
                        }
                    });
                    let entries = videos
                        .into_iter()
                        .map(|video| MediaEntry {
                            url: video.url,
                            extension: Some("mp4".to_string()),
                            duration_secs: video.duration,
                            size_bytes: video.size,
                            quality: video.quality_type,
                            width: video.width,
                            height: video.height,
                        })
                        .collect();
                    return ExtractionResult::success(
                        url,
                        MediaPlatform::Xhs,
                        MediaKind::Video,
                        non_empty_title(&title),
                        entries,
                    );
                }
                if note.images.is_empty() {
                    return ExtractionResult::failure(
                        url,
                        MediaPlatform::Xhs,
                        "未找到可下载的视频或图片",
                    );
                }
                let entries = note
                    .images
                    .iter()
                    .flat_map(|image| {
                        let mut entries = vec![MediaEntry {
                            url: image.raw_url.clone(),
                            extension: image
                                .extension
                                .clone()
                                .or_else(|| Some("jpg".to_string())),
                            size_bytes: self.http.head_content_length(&image.raw_url),
                            width: image.width,
                            height: image.height,
                            ..Default::default()
                        }];
                        for video in &image.live_videos {
                            entries.push(MediaEntry {
                                url: video.url.clone(),
                                extension: Some("mp4".to_string()),
                                duration_secs: video.duration,
                                size_bytes: video.size,
                                quality: Some("LIVE".to_string()),
                                width: video.width,
                                height: video.height,
                            });
                        }
                        entries
                    })
                    .collect();
                ExtractionResult::success(
                    url,
                    MediaPlatform::Xhs,
                    MediaKind::Image,
                    non_empty_title(&title),
                    entries,
                )
            }
            Err(error) => ExtractionResult::failure(
                url,
                MediaPlatform::Xhs,
                format!("解析失败: {error}"),
            ),
        }
    }

    fn extract_douyin(&self, url: &str) -> ExtractionResult {
        let mut page_url = url.to_string();
        let mut aweme_id = url_parser::extract_aweme_id(url);
        if aweme_id.is_none() && url.contains("v.douyin.com") {
            let mut current = url.to_string();
            for _ in 0..5 {
                if aweme_id.is_some() {
                    break;
                }
                let Some(redirected) = self.http.get_redirect_url(&current) else {
                    break;
                };
                current = if redirected.starts_with("http") {
                    redirected
                } else {
                    match url::Url::parse(&current).and_then(|base| base.join(&redirected)) {
                        Ok(resolved) => resolved.to_string(),
                        Err(_) => break,
                    }
                };
                page_url = current.clone();
                aweme_id = url_parser::extract_aweme_id(&current);
            }
        }

        let Some(aweme_id) = aweme_id else {
            return ExtractionResult::failure(url, MediaPlatform::Douyin, "链接无效或格式不正确");
        };

        // 首选：签名接口（不依赖页面结构）
        if let Some(detail) = self.api.fetch_aweme_detail(&aweme_id) {
            let Some(video_url) = detail.play_urls.first().cloned() else {
                return ExtractionResult::failure(
                    url,
                    MediaPlatform::Douyin,
                    "无法获取视频播放链接",
                );
            };
            let size_bytes = detail.size_bytes.or_else(|| self.http.head_content_length(&video_url));
            let entry = MediaEntry {
                url: video_url,
                extension: None,
                duration_secs: detail.duration_ms.map(|ms| ms / 1000),
                size_bytes,
                quality: gear_quality(detail.quality.as_deref()),
                width: detail.width,
                height: detail.height,
            };
            return ExtractionResult::success(
                url,
                MediaPlatform::Douyin,
                MediaKind::Video,
                detail.desc,
                vec![entry],
            );
        }

        // 回退：分享页 HTML 流程
        let html = match self.http.fetch_html(&page_url) {
            Ok(html) => html,
            Err(err) => return ExtractionResult::failure(url, MediaPlatform::Douyin, format!("解析失败: {err}")),
        };
        let Some(video_id) = url_parser::extract_video_id(&html) else {
            return ExtractionResult::failure(url, MediaPlatform::Douyin, "无法获取视频播放链接");
        };
        let video_title = url_parser::extract_video_title(&html);
        let Some(final_url) = self.http.get_final_video_url(&video_id) else {
            return ExtractionResult::failure(url, MediaPlatform::Douyin, "无法获取视频播放链接");
        };
        let size_bytes = self.http.head_content_length(&final_url);
        let entry = MediaEntry {
            url: final_url,
            size_bytes,
            ..Default::default()
        };
        ExtractionResult::success(
            url,
            MediaPlatform::Douyin,
            MediaKind::Video,
            video_title,
            vec![entry],
        )
    }
}

/// gear_name 形如 `adapt_720_1080`、`normal_540_720`；提取分辨率
/// 数字中较大者显示为 `1080P`。无法提取时返回 None。
fn gear_quality(gear: Option<&str>) -> Option<String> {
    let gear = gear?;
    let numbers: Vec<u64> = gear
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    let mut pairs = numbers.chunks(2).filter(|pair| pair.len() == 2);
    let best = pairs.next()?.iter().max()?;
    // 分辨率数字至少是 240，避免把 `adapt_lowest_4_1` 之类误判为清晰度。
    if *best < 144 {
        return None;
    }
    Some(format!("{best}P"))
}

fn non_empty_title(title: &str) -> Option<String> {
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}
