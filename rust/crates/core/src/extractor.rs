//! 解析编排：链接解析、接口请求与播放地址提取。

use crate::api::{AwemeDetail, DouyinApi};
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
    /// 清晰度标签（xhs: 原画/HD；抖音: 原画/2160P 等）。
    pub quality: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    /// 预览缩略图（图片为展示图，视频为视频封面）。
    pub thumbnail_url: Option<String>,
    /// 缩略图宽高；视频卡片按它（封面比例）展示，避免被视频分辨率拉窄。
    pub thumbnail_width: Option<u64>,
    pub thumbnail_height: Option<u64>,
    /// 封面原图（封面标签页复制/下载使用）。
    pub cover_url: Option<String>,
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
    /// 视频封面（体验版轮播展示与封面下载使用），可能有多张。
    pub covers: Vec<MediaEntry>,
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
            covers: Vec::new(),
            error: Some(error.into()),
        }
    }

    fn success(
        original_url: &str,
        platform: MediaPlatform,
        media_kind: MediaKind,
        title: Option<String>,
        entries: Vec<MediaEntry>,
        covers: Vec<MediaEntry>,
    ) -> Self {
        Self {
            success: true,
            original_url: original_url.to_string(),
            platform,
            media_kind,
            title,
            entries,
            covers,
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
                let title = xhs_display_title(&note.title, &note.desc);
                if !note.videos.is_empty() {
                    // 小红书视频笔记的 imageList[0] 即视频封面。
                    let cover_thumbnail =
                        note.images.first().map(|image| image.preview_url.clone());
                    let cover_original = note.images.first().map(|image| {
                        if image.raw_url.is_empty() {
                            image.url.clone()
                        } else {
                            image.raw_url.clone()
                        }
                    });
                    let cover_size = note.images.first().map(|image| (image.width, image.height));
                    let cover = note.images.first().map(|image| {
                        let original = if image.raw_url.is_empty() {
                            image.url.clone()
                        } else {
                            image.raw_url.clone()
                        };
                        MediaEntry {
                            url: original.clone(),
                            extension: image.extension.clone().or_else(|| Some("jpg".to_string())),
                            size_bytes: image.size_bytes,
                            width: image.width,
                            height: image.height,
                            thumbnail_url: Some(image.preview_url.clone()),
                            cover_url: Some(original),
                            ..Default::default()
                        }
                    });
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
                            quality: xhs_quality(video.quality_type.as_deref()),
                            width: video.width,
                            height: video.height,
                            thumbnail_url: cover_thumbnail.clone(),
                            thumbnail_width: cover_size.and_then(|(width, _)| width),
                            thumbnail_height: cover_size.and_then(|(_, height)| height),
                            cover_url: cover_original.clone(),
                        })
                        .collect();
                    return ExtractionResult::success(
                        url,
                        MediaPlatform::Xhs,
                        MediaKind::Video,
                        non_empty_title(&title),
                        entries,
                        cover.into_iter().collect(),
                    );
                }
                if note.images.is_empty() {
                    return ExtractionResult::failure(
                        url,
                        MediaPlatform::Xhs,
                        "未找到可下载的视频或图片",
                    );
                }
                let raw_urls: Vec<&str> = note
                    .images
                    .iter()
                    .map(|image| image.raw_url.as_str())
                    .collect();
                let sizes = self.http.head_content_length_batch(&raw_urls);
                let entries = note
                    .images
                    .iter()
                    .zip(sizes)
                    .flat_map(|(image, size_bytes)| {
                        let mut entries = vec![MediaEntry {
                            url: image.raw_url.clone(),
                            extension: image.extension.clone().or_else(|| Some("jpg".to_string())),
                            size_bytes,
                            width: image.width,
                            height: image.height,
                            thumbnail_url: Some(image.preview_url.clone()),
                            cover_url: Some(image.raw_url.clone()),
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
                                ..Default::default()
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
                    Vec::new(),
                )
            }
            Err(error) => ExtractionResult::failure(url, MediaPlatform::Xhs, error),
        }
    }

    /// 图文 / 实况作品：每张图一条，实况图紧跟其动轨视频，
    /// 动轨与小红书一致标记 `LIVE`。
    fn image_entries(&self, detail: &AwemeDetail) -> Vec<MediaEntry> {
        let urls: Vec<&str> = detail
            .images
            .iter()
            .map(|image| image.url.as_str())
            .collect();
        let sizes = self.http.head_content_length_batch(&urls);
        let mut entries = Vec::new();
        for (image, size_bytes) in detail.images.iter().zip(sizes) {
            entries.push(MediaEntry {
                url: image.url.clone(),
                extension: image_extension(&image.url),
                size_bytes,
                width: image.width,
                height: image.height,
                thumbnail_url: Some(image.url.clone()),
                cover_url: Some(image.url.clone()),
                ..Default::default()
            });
            let Some(live) = image.live_video.as_ref() else {
                continue;
            };
            let original_url = live
                .play_uri
                .as_deref()
                .and_then(|uri| self.http.get_final_video_url(uri))
                .filter(|url| is_original_video_url(url));
            let (video_url, is_original) = match original_url {
                Some(original_url) => (original_url, true),
                None => {
                    let Some(live_url) = live.play_urls.first().cloned() else {
                        continue;
                    };
                    (live_url, false)
                }
            };
            let size_bytes = if is_original {
                self.http.head_content_length(&video_url)
            } else {
                live.size_bytes
                    .or_else(|| self.http.head_content_length(&video_url))
            };
            entries.push(MediaEntry {
                url: video_url,
                extension: Some("mp4".to_string()),
                duration_secs: live.duration_ms.map(|ms| ms / 1000),
                size_bytes,
                quality: Some("LIVE".to_string()),
                width: live.width,
                height: live.height,
                ..Default::default()
            });
        }
        entries
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
            // 图文 / 实况作品：图片逐张输出，实况图额外带上对应的视频；
            // 视频若存在原画源（aweme/v1/play 解析到 tos-cn-v 原画）再补一条“原画”。
            if !detail.images.is_empty() {
                let mut entries = self.image_entries(&detail);
                if let Some(music) = &detail.music {
                    entries.push(MediaEntry {
                        url: music.url.clone(),
                        extension: Some("mp3".to_string()),
                        duration_secs: music.duration_secs,
                        size_bytes: self.http.head_content_length(&music.url),
                        quality: Some("背景音乐".to_string()),
                        ..Default::default()
                    });
                }
                if entries.is_empty() {
                    return ExtractionResult::failure(
                        url,
                        MediaPlatform::Douyin,
                        "未找到可下载的图片或视频",
                    );
                }
                return ExtractionResult::success(
                    url,
                    MediaPlatform::Douyin,
                    MediaKind::Image,
                    detail.desc.clone(),
                    entries,
                    Vec::new(),
                );
            }
            // 原画优先：play_addr.uri 通过 aweme/v1/play 会重定向到
            // tos-cn-v-0015c002 原画（通常为 4K/H.265，远大于 web 播放地址）。
            // 接口失败时回退到 web play_addr。
            let original_url = detail
                .play_uri
                .as_deref()
                .and_then(|video_id| self.http.get_final_video_url(video_id))
                .filter(|video_url| !video_url.contains("aweme.snssdk.com"));
            let (video_url, is_original) = match original_url {
                Some(video_url) => (video_url, true),
                None => {
                    let Some(video_url) = detail.play_urls.first().cloned() else {
                        return ExtractionResult::failure(
                            url,
                            MediaPlatform::Douyin,
                            "无法获取视频播放链接",
                        );
                    };
                    (video_url, false)
                }
            };
            let duration_secs = detail.duration_ms.map(|ms| ms / 1000);
            // 手机端与 PC 端封面不同：dynamic_cover 是手机展示的高清原图，
            // cover 是 PC 端展示的裁切图，两张都放进轮播。
            let primary_cover = detail
                .dynamic_cover
                .clone()
                .or_else(|| detail.origin_cover.clone())
                .or_else(|| detail.cover.clone());
            let pc_cover = detail.cover.clone();
            let primary_dimensions = primary_cover
                .as_deref()
                .and_then(|url| self.http.image_dimensions(url));
            let pc_dimensions = if pc_cover.as_deref() == primary_cover.as_deref() {
                primary_dimensions
            } else {
                pc_cover
                    .as_deref()
                    .and_then(|url| self.http.image_dimensions(url))
            };
            let build_cover =
                |url: &str, dimensions: Option<(u64, u64)>, known_size: Option<u64>| {
                    let (size_bytes, content_type) = self.http.head_meta(url);
                    MediaEntry {
                        url: url.to_string(),
                        extension: extension_from_content_type(content_type.as_deref())
                            .or_else(|| image_extension(url))
                            .or_else(|| Some("jpg".to_string())),
                        size_bytes: known_size.or(size_bytes),
                        width: dimensions
                            .map(|(width, _)| width)
                            .or(detail.cover_width)
                            .or(detail.width),
                        height: dimensions
                            .map(|(_, height)| height)
                            .or(detail.cover_height)
                            .or(detail.height),
                        thumbnail_url: Some(url.to_string()),
                        cover_url: Some(url.to_string()),
                        ..Default::default()
                    }
                };
            let mut covers = Vec::new();
            if let Some(url) = primary_cover.as_deref() {
                covers.push(build_cover(
                    url,
                    primary_dimensions,
                    detail.cover_size_bytes,
                ));
            }
            if let Some(url) = pc_cover.as_deref() {
                if !covers.iter().any(|cover| cover.url == url) {
                    covers.push(build_cover(url, pc_dimensions, None));
                }
            }
            // 视频卡片的预览用 PC 封面（origin_cover 可能是黑帧）。
            let cover_thumbnail = pc_cover.clone().or_else(|| primary_cover.clone());
            let thumbnail_dimensions = if pc_cover.is_some() {
                pc_dimensions
            } else {
                primary_dimensions
            };
            let cover_original = primary_cover.clone();
            let mut entries = Vec::new();
            if is_original {
                entries.push(MediaEntry {
                    url: video_url.clone(),
                    extension: None,
                    duration_secs,
                    size_bytes: self.http.head_content_length(&video_url),
                    quality: Some("原画".to_string()),
                    width: detail.width,
                    height: detail.height,
                    thumbnail_url: cover_thumbnail.clone(),
                    thumbnail_width: thumbnail_dimensions
                        .map(|(width, _)| width)
                        .or(detail.cover_width),
                    thumbnail_height: thumbnail_dimensions
                        .map(|(_, height)| height)
                        .or(detail.cover_height),
                    cover_url: cover_original.clone(),
                });
            } else {
                let matched = detail
                    .bit_rates
                    .iter()
                    .find(|bit_rate| bit_rate.url.as_deref() == Some(video_url.as_str()));
                entries.push(MediaEntry {
                    url: video_url.clone(),
                    extension: None,
                    duration_secs,
                    size_bytes: matched
                        .and_then(|bit_rate| bit_rate.size_bytes)
                        .or_else(|| self.http.head_content_length(&video_url)),
                    quality: matched
                        .and_then(|bit_rate| {
                            bit_rate_quality(bit_rate.quality.as_deref(), bit_rate.height)
                        })
                        .or_else(|| gear_quality(detail.quality.as_deref())),
                    width: matched.and_then(|bit_rate| bit_rate.width),
                    height: matched.and_then(|bit_rate| bit_rate.height),
                    thumbnail_url: cover_thumbnail.clone(),
                    thumbnail_width: thumbnail_dimensions
                        .map(|(width, _)| width)
                        .or(detail.cover_width),
                    thumbnail_height: thumbnail_dimensions
                        .map(|(_, height)| height)
                        .or(detail.cover_height),
                    cover_url: cover_original.clone(),
                });
            }

            let mut seen_resolutions: Vec<(u64, u64)> = Vec::new();
            for bit_rate in &detail.bit_rates {
                let Some(bit_rate_url) = bit_rate.url.clone() else {
                    continue;
                };
                let (Some(width), Some(height)) = (bit_rate.width, bit_rate.height) else {
                    continue;
                };
                if seen_resolutions.contains(&(width, height)) {
                    continue;
                }
                if entries.iter().any(|entry| entry.url == bit_rate_url) {
                    continue;
                }
                seen_resolutions.push((width, height));
                entries.push(MediaEntry {
                    url: bit_rate_url.clone(),
                    extension: None,
                    duration_secs,
                    size_bytes: bit_rate
                        .size_bytes
                        .or_else(|| self.http.head_content_length(&bit_rate_url)),
                    quality: bit_rate_quality(bit_rate.quality.as_deref(), Some(height)),
                    width: Some(width),
                    height: Some(height),
                    thumbnail_url: cover_thumbnail.clone(),
                    thumbnail_width: thumbnail_dimensions
                        .map(|(width, _)| width)
                        .or(detail.cover_width),
                    thumbnail_height: thumbnail_dimensions
                        .map(|(_, height)| height)
                        .or(detail.cover_height),
                    cover_url: cover_original.clone(),
                });
            }

            if entries.len() > 1 {
                let first = entries.remove(0);
                entries.sort_by_key(|entry| {
                    (
                        std::cmp::Reverse(entry.height.unwrap_or(0)),
                        std::cmp::Reverse(entry.width.unwrap_or(0)),
                    )
                });
                entries.insert(0, first);
            }

            return ExtractionResult::success(
                url,
                MediaPlatform::Douyin,
                MediaKind::Video,
                detail.desc,
                entries,
                covers,
            );
        }

        // 回退：分享页 HTML 流程
        let html = match self.http.fetch_html(&page_url) {
            Ok(html) => html,
            Err(err) => return ExtractionResult::failure(url, MediaPlatform::Douyin, err),
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
            Vec::new(),
        )
    }
}

fn extension_from_content_type(content_type: Option<&str>) -> Option<String> {
    let mime = content_type?.split(';').next()?.trim().to_ascii_lowercase();
    match mime.as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg".to_string()),
        "image/png" => Some("png".to_string()),
        "image/webp" => Some("webp".to_string()),
        "image/gif" => Some("gif".to_string()),
        "image/avif" => Some("avif".to_string()),
        "image/heic" => Some("heic".to_string()),
        _ => None,
    }
}

fn image_extension(url: &str) -> Option<String> {
    let path = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    let extension = path.rsplit('.').next()?;
    match extension {
        "jpg" | "jpeg" => Some("jpg".to_string()),
        "png" | "webp" | "gif" | "heic" | "avif" => Some(extension.to_string()),
        _ => None,
    }
}

/// 抖音原画源路径形如 `/video/tos/cn/tos-cn-v-0015c002/...`；
/// 普通 web 清晰度为 `tos-cn-ve-...`，不视为原画。
fn is_original_video_url(url: &str) -> bool {
    url.contains("tos-cn-v-")
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

fn bit_rate_quality(gear: Option<&str>, height: Option<u64>) -> Option<String> {
    gear_quality(gear).or_else(|| height.map(|height| format!("{height}P")))
}

fn xhs_quality(quality: Option<&str>) -> Option<String> {
    quality.map(|quality| {
        if quality == "RAW" {
            "原画".to_string()
        } else {
            quality.to_string()
        }
    })
}

fn non_empty_title(title: &str) -> Option<String> {
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// 小红书网页标题 = 标题 + 正文；接口的 `title` 常只有第一行。
/// 正文以标题开头时直接使用正文，避免重复。
fn xhs_display_title(title: &str, desc: &str) -> String {
    let title = title.trim();
    let desc = desc.trim();
    match (title.is_empty(), desc.is_empty()) {
        (true, _) => desc.to_string(),
        (false, true) => title.to_string(),
        (false, false) if desc.starts_with(title) => desc.to_string(),
        (false, false) => format!("{title}\n{desc}"),
    }
}

#[cfg(test)]
mod tests {
    use super::xhs_display_title;

    #[test]
    fn xhs_title_without_desc_keeps_title() {
        assert_eq!(xhs_display_title("标题", ""), "标题");
    }

    #[test]
    fn xhs_desc_starting_with_title_is_not_duplicated() {
        assert_eq!(
            xhs_display_title("墨镜舞教程可以看这里", "墨镜舞教程可以看这里\n正文"),
            "墨镜舞教程可以看这里\n正文"
        );
    }

    #[test]
    fn xhs_title_and_desc_are_joined_with_newline() {
        assert_eq!(
            xhs_display_title("墨镜舞教程可以看这里", "张列车长包教包会😎"),
            "墨镜舞教程可以看这里\n张列车长包教包会😎"
        );
    }
}
