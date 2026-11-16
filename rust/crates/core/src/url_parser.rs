//! 链接提取与页面解析。

use regex::Regex;
use std::sync::LazyLock;

static SHORT_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)https?://v\.douyin\.com/[A-Za-z0-9_-]+").unwrap());

static PAGE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)https?://[a-z0-9.-]*douyin\.com/[^\s"'<>]+"#).unwrap());

static SHARE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)https?://(?:www\.)?iesdouyin\.com/share/(?:video|note)/(\d+)").unwrap()
});

static XHS_SHORT_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)https?://xhslink\.(?:com|cn)/[/\w]+").unwrap());

static XHS_PAGE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)https?://[\w.-]*xiaohongshu\.com/(?:explore|discovery/item|user/profile)/[^\s'<>]+",
    )
    .unwrap()
});

static TRAILING_PUNCTUATION: &[char] = &[
    '，', '。', '；', '：', '！', '？', '、', ',', ';', ':', '!', '?',
];

static MODAL_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)modal_id=(\d+)").unwrap());

static VIDEO_OR_NOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)/(?:video|note)/(\d+)").unwrap());

static VIDEO_URL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https?://(?:www\.)?douyin\.com/video/(\d+)/?$").unwrap());

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|item| item == value) {
        list.push(value.to_string());
    }
}

fn host_of(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://")?.1;
    Some(after_scheme.split('/').next()?)
}

fn is_short_link(url: &str) -> bool {
    SHORT_LINK.is_match(url)
}

/// 从文本中提取所有抖音视频链接，并归一化为分享页地址。
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();

    // 小红书链接保持原样（短链需要在解析时跟随重定向，页面链接需要保留 xsec_token）。
    for m in XHS_SHORT_LINK.find_iter(text) {
        push_unique(&mut urls, m.as_str());
    }
    for m in XHS_PAGE_LINK.find_iter(text) {
        push_unique(&mut urls, trim_trailing_punctuation(m.as_str()));
    }

    for m in SHORT_LINK.find_iter(text) {
        push_unique(&mut urls, m.as_str());
    }
    for m in PAGE_LINK.find_iter(text) {
        let matched = m.as_str();
        // 短链（v.douyin.com）由 SHORT_LINK 处理，避免重复。
        if host_of(matched).is_some_and(|host| host.starts_with("v.")) {
            continue;
        }
        push_unique(&mut urls, matched);
    }
    for m in SHARE_LINK.find_iter(text) {
        push_unique(&mut urls, m.as_str());
    }

    let mut result = Vec::new();
    for url in urls {
        if crate::xhs::is_xhs_url(&url) {
            push_unique(&mut result, &url);
            continue;
        }
        if is_short_link(&url) {
            push_unique(&mut result, &url);
            continue;
        }
        if let Some(id) = extract_aweme_id(&url) {
            push_unique(
                &mut result,
                &format!("https://www.iesdouyin.com/share/video/{id}/"),
            );
        }
    }
    result
}

fn trim_trailing_punctuation(url: &str) -> &str {
    url.trim_end_matches(TRAILING_PUNCTUATION)
}

/// 从链接中提取数字视频 ID（`modal_id=` / `video/{id}` / `note/{id}`）。
pub fn extract_aweme_id(url: &str) -> Option<String> {
    if let Some(caps) = MODAL_ID.captures(url) {
        return Some(caps[1].to_string());
    }
    if let Some(caps) = VIDEO_OR_NOTE.captures(url) {
        return Some(caps[1].to_string());
    }
    None
}

/// 提取第一个链接（向后兼容 `extractUrl`）。
pub fn extract_url(text: &str) -> Option<String> {
    extract_urls(text).into_iter().next()
}

pub fn convert_douyin_video_url(url: &str) -> String {
    if let Some(caps) = VIDEO_URL_PATTERN.captures(url) {
        return format!("https://www.iesdouyin.com/share/video/{}/", &caps[1]);
    }
    url.to_string()
}

/// 从分享页 HTML 提取 `video_id`（支持多种匹配方式）。
pub fn extract_video_id(html: &str) -> Option<String> {
    static RE1: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"video_id=([a-zA-Z0-9_\-]+)").unwrap());
    static RE2: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#""video_id"\s*:\s*"([^"]+)""#).unwrap());
    static RE3: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"'video_id'\s*:\s*'([^']+)'").unwrap());
    static RE4: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"video_id[=:]([a-zA-Z0-9_\-]{15,})").unwrap());
    static RE5: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"video_id=([a-zA-Z0-9_\-]{15,})").unwrap());

    if let Some(caps) = RE1.captures(html) {
        return Some(caps[1].to_string());
    }
    if let Some(caps) = RE2.captures(html) {
        return Some(caps[1].to_string());
    }
    if let Some(caps) = RE3.captures(html) {
        return Some(caps[1].to_string());
    }
    if let Some(caps) = RE4.captures(html) {
        return Some(caps[1].to_string());
    }
    let decoded = html
        .replace(r"\\u002F", "/")
        .replace(r"\\/", "/")
        .replace(r#"\""#, "\"")
        .replace(r"\'", "'");
    RE5.captures(&decoded).map(|caps| caps[1].to_string())
}

/// 从 HTML 提取标题（title / og:title / desc）。
pub fn extract_video_title(html: &str) -> Option<String> {
    static TITLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap());
    static OG_TITLE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<meta\s+property="og:title"\s+content="([^"]+)""#).unwrap()
    });
    static DESC: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?is)"desc(?:ription)?"\s*:\s*"([^"]+)""#).unwrap());

    for re in [&*TITLE, &*OG_TITLE, &*DESC] {
        if let Some(caps) = re.captures(html) {
            let title = caps[1].trim();
            if !title.is_empty() {
                return Some(clean_title(title));
            }
        }
    }
    None
}

/// 清理标题：去掉抖音后缀与非法字符，不截断显示/复制内容。
pub fn clean_title(title: &str) -> String {
    static SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*-?\s*抖音.*$").unwrap());
    static RECORD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\s*-?\s*记录美好生活$").unwrap());
    static HASH_DOUYIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*#抖音.*$").unwrap());
    static ILLEGAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[<>:"/\\|?*]"#).unwrap());
    static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let mut cleaned = title.trim().to_string();
    cleaned = SUFFIX.replace_all(&cleaned, "").to_string();
    cleaned = RECORD.replace_all(&cleaned, "").to_string();
    cleaned = HASH_DOUYIN.replace_all(&cleaned, "").to_string();
    cleaned = ILLEGAL.replace_all(&cleaned, "").to_string();
    cleaned = SPACES.replace_all(&cleaned, " ").to_string();
    cleaned = cleaned.trim().to_string();

    if cleaned.is_empty() {
        "抖音视频".to_string()
    } else {
        cleaned
    }
}

pub fn generate_video_url(video_id: &str) -> String {
    format!(
        "https://aweme.snssdk.com/aweme/v1/play/?video_id={video_id}&ratio=default&line=0&h265=1"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_normalizes_links() {
        let text = "5.12 复制打开抖音 https://v.douyin.com/AbC-12/ 看看 \
                    https://www.douyin.com/video/7588361322312650038 和 \
                    https://www.douyin.com/jingxuan?modal_id=7701193143558901018 还有 \
                    https://www.iesdouyin.com/share/video/7588361322312650038";
        let urls = extract_urls(text);
        assert_eq!(
            urls,
            vec![
                "https://v.douyin.com/AbC-12".to_string(),
                "https://www.iesdouyin.com/share/video/7588361322312650038/".to_string(),
                "https://www.iesdouyin.com/share/video/7701193143558901018/".to_string(),
            ]
        );
    }

    #[test]
    fn extracts_video_id_and_title() {
        let html = r#"<html><head><title>今天天气不错 - 抖音</title></head>
            <script>var x = {"video_id":"v0200fg10000abc"};
            </script></html>"#;
        assert_eq!(extract_video_id(html).as_deref(), Some("v0200fg10000abc"));
        assert_eq!(extract_video_title(html).as_deref(), Some("今天天气不错"));
    }

    #[test]
    fn clean_title_strips_without_truncating() {
        // 先去掉「抖音…」后缀，残留的 # 保留
        assert_eq!(clean_title("hello #抖音 something"), "hello #");
        assert_eq!(clean_title(&"a".repeat(200)), "a".repeat(200));
        assert_eq!(clean_title("   "), "抖音视频");
    }
}
