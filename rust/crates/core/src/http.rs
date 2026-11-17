//! HTML、重定向与播放地址请求。

use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
    COOKIE, LOCATION, RANGE, SET_COOKIE, USER_AGENT,
};
use url::Url;

pub const IOS_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Mobile/15E148 Safari/604.1";

pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .http1_only()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { client })
    }

    /// 手动跟随最多 5 次重定向后返回 HTML。
    pub fn fetch_html(&self, url: &str) -> Result<String, String> {
        self.fetch_html_with(url, &html_headers())
    }

    /// 使用自定义请求头手动跟随最多 5 次重定向后返回 HTML。
    pub fn fetch_html_with(&self, url: &str, headers: &HeaderMap) -> Result<String, String> {
        self.fetch_html_ex(url, headers, None).map(|(html, _)| html)
    }

    /// 使用自定义请求头与可选 Cookie 手动跟随最多 5 次重定向，
    /// 返回 HTML 与响应中的 Cookie（"name=value; ..."）。
    pub fn fetch_html_ex(
        &self,
        url: &str,
        headers: &HeaderMap,
        cookie: Option<&str>,
    ) -> Result<(String, String), String> {
        let mut current = url.to_string();
        for _ in 0..5 {
            let mut request = self.client.get(&current).headers(headers.clone());
            if let Some(cookie) = cookie {
                request = request.header(COOKIE, cookie);
            }
            let response = request
                .send()
                .map_err(|_| "网络请求异常，请在系统设置中允许本应用使用网络".to_string())?;
            let status = response.status().as_u16();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string);
                let Some(location) = location else {
                    return Err("重定向响应中没有Location头".to_string());
                };
                current = resolve_url(&current, &location)?;
                continue;
            }
            if status == 200 {
                let set_cookie = collect_set_cookies(&response);
                let html = response
                    .text()
                    .map_err(|err| format!("网络请求异常: {err}"))?;
                return Ok((html, set_cookie));
            }
            return Err(format!("HTTP请求失败: {status}"));
        }
        Err("达到最大重定向次数: 5".to_string())
    }

    /// HEAD 请求取 301/302 的 Location。
    pub fn get_redirect_url(&self, url: &str) -> Option<String> {
        let response = self.client.head(url).headers(ios_headers()).send().ok()?;
        if matches!(response.status().as_u16(), 301 | 302) {
            return response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        None
    }

    /// HEAD 请求取响应体的 Content-Length。
    pub fn head_content_length(&self, url: &str) -> Option<u64> {
        self.head_meta(url).0
    }

    /// HEAD 请求同时取 Content-Length 与 Content-Type。
    ///
    /// 部分 CDN 对 HEAD 返回 403/405 或省略 Content-Length（chunked），
    /// 此时用 `Range: bytes=0-0` 的 GET 兜底，从 Content-Range 读取总大小。
    pub fn head_meta(&self, url: &str) -> (Option<u64>, Option<String>) {
        let (length, content_type) = self.head_meta_follow(url);
        if length.is_some() {
            return (length, content_type);
        }
        let (range_length, range_type) = self.range_meta_follow(url);
        (range_length.or(length), content_type.or(range_type))
    }

    /// 分批并行探测多个地址，避免图文笔记逐张等待造成解析缓慢。
    pub fn head_meta_batch(&self, urls: &[&str]) -> Vec<(Option<u64>, Option<String>)> {
        let mut results = vec![(None, None); urls.len()];
        if urls.is_empty() {
            return results;
        }
        let workers = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(4)
            .clamp(1, 8)
            .min(urls.len());
        let chunk_size = urls.len().div_ceil(workers);
        std::thread::scope(|scope| {
            for (urls, results) in urls.chunks(chunk_size).zip(results.chunks_mut(chunk_size)) {
                scope.spawn(move || {
                    for (url, result) in urls.iter().zip(results.iter_mut()) {
                        *result = self.head_meta(url);
                    }
                });
            }
        });
        results
    }

    pub fn head_content_length_batch(&self, urls: &[&str]) -> Vec<Option<u64>> {
        self.head_meta_batch(urls)
            .into_iter()
            .map(|(length, _)| length)
            .collect()
    }

    fn head_meta_follow(&self, url: &str) -> (Option<u64>, Option<String>) {
        let mut current = url.to_string();
        for _ in 0..5 {
            let Ok(response) = self.client.head(&current).headers(ios_headers()).send() else {
                return (None, None);
            };
            let status = response.status().as_u16();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let Some(next) = redirect_target(&current, &response) else {
                    return (None, None);
                };
                current = next;
                continue;
            }
            if !response.status().is_success() {
                return (None, None);
            }
            return (content_length(&response), content_type(&response));
        }
        (None, None)
    }

    fn range_meta_follow(&self, url: &str) -> (Option<u64>, Option<String>) {
        let mut current = url.to_string();
        for _ in 0..5 {
            let Ok(response) = self
                .client
                .get(&current)
                .headers(ios_headers())
                .header(RANGE, "bytes=0-0")
                .send()
            else {
                return (None, None);
            };
            let status = response.status().as_u16();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let Some(next) = redirect_target(&current, &response) else {
                    return (None, None);
                };
                current = next;
                continue;
            }
            let content_type = content_type(&response);
            if status == 206 {
                let total = response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(content_range_total);
                return (total, content_type);
            }
            if response.status().is_success() {
                return (content_length(&response), content_type);
            }
            return (None, None);
        }
        (None, None)
    }

    /// 读取图片头部解析真实宽高（封面接口里的 width/height 并不可靠）。
    pub fn image_dimensions(&self, url: &str) -> Option<(u64, u64)> {
        let bytes = self.fetch_byte_range(url, 0, 32767, 32 * 1024)?;
        parse_image_dimensions(&bytes)
    }

    /// Range 请求获取响应体，最多读取 `max_bytes` 字节。
    pub fn fetch_byte_range(
        &self,
        url: &str,
        start: u64,
        end_inclusive: u64,
        max_bytes: usize,
    ) -> Option<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .headers(ios_headers())
            .header(RANGE, format!("bytes={start}-{end_inclusive}"))
            .send()
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let mut body = Vec::new();
        response
            .take(max_bytes as u64)
            .read_to_end(&mut body)
            .ok()?;
        Some(body)
    }

    /// 通过 aweme 播放接口拿到最终视频地址。
    pub fn get_final_video_url(&self, video_id: &str) -> Option<String> {
        let url = crate::url_parser::generate_video_url(video_id);
        if let Some(location) = self.get_redirect_url(&url) {
            return Some(ensure_dy_q(location));
        }
        let response = self.client.get(&url).headers(ios_headers()).send().ok()?;
        if matches!(response.status().as_u16(), 301 | 302) {
            return response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(|value| ensure_dy_q(value.to_string()));
        }
        Some(url)
    }
}

/// CDN 地址有时缺少 `dy_q` 参数，补上当前时间戳可避免部分网络下
/// 直接访问或下载失败。
fn ensure_dy_q(url: String) -> String {
    let Ok(mut parsed) = Url::parse(&url) else {
        return url;
    };
    if parsed.query_pairs().any(|(key, _)| key == "dy_q") {
        return url;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    parsed
        .query_pairs_mut()
        .append_pair("dy_q", &timestamp.to_string())
        .append_pair("dy_va_biz_cert", "");
    parsed.to_string()
}

fn collect_set_cookies(response: &reqwest::blocking::Response) -> String {
    let mut cookies = Vec::new();
    for value in response.headers().get_all(SET_COOKIE) {
        let Ok(value) = value.to_str() else {
            continue;
        };
        let pair = value.split(';').next().unwrap_or("").trim();
        if pair.contains('=') {
            cookies.push(pair.to_string());
        }
    }
    cookies.join("; ")
}

fn ios_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(IOS_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9"));
    headers
}

fn html_headers() -> HeaderMap {
    let mut headers = ios_headers();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7",
        ),
    );
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("deflate"),
    );
    headers
}

fn resolve_url(base: &str, location: &str) -> Result<String, String> {
    if location.starts_with("http") {
        return Ok(location.to_string());
    }
    Url::parse(base)
        .and_then(|url| url.join(location))
        .map(|url| url.to_string())
        .map_err(|err| format!("重定向地址解析失败: {err}"))
}

fn redirect_target(current: &str, response: &reqwest::blocking::Response) -> Option<String> {
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())?;
    resolve_url(current, location).ok()
}

fn content_length(response: &reqwest::blocking::Response) -> Option<u64> {
    response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|length| *length > 0)
}

fn content_type(response: &reqwest::blocking::Response) -> Option<String> {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// 解析 `bytes 0-0/12345` 中的总大小；`*` 表示未知。
fn content_range_total(value: &str) -> Option<u64> {
    value
        .rsplit_once('/')
        .and_then(|(_, total)| total.trim().parse::<u64>().ok())
        .filter(|total| *total > 0)
}

fn parse_image_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return jpeg_dimensions(bytes);
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
        let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
        return Some((u64::from(width), u64::from(height)));
    }
    if bytes.len() >= 16 && bytes.starts_with(b"RIFF") && bytes.get(8..12)? == b"WEBP" {
        return webp_dimensions(bytes);
    }
    None
}

/// 扫描 JPEG 的 SOF 段读取宽高（跳过 APPn/EXIF 等其他段）。
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    let mut index = 2;
    while index + 8 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        if marker == 0xFF {
            index += 1;
            continue;
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            index += 2;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([bytes[index + 2], bytes[index + 3]]));
        let is_sof = (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            let height = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]);
            let width = u16::from_be_bytes([bytes[index + 7], bytes[index + 8]]);
            return Some((u64::from(width), u64::from(height)));
        }
        index += 2 + length;
    }
    None
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    match bytes.get(12..16)? {
        b"VP8X" => {
            let width = 1 + read_le24(bytes.get(24..27)?);
            let height = 1 + read_le24(bytes.get(27..30)?);
            Some((width, height))
        }
        b"VP8 " => {
            let start = bytes.get(23..26)?;
            if start != [0x9D, 0x01, 0x2A] {
                return None;
            }
            let width = u64::from(u16::from_le_bytes([*bytes.get(26)?, *bytes.get(27)?]) & 0x3FFF);
            let height = u64::from(u16::from_le_bytes([*bytes.get(28)?, *bytes.get(29)?]) & 0x3FFF);
            Some((width, height))
        }
        b"VP8L" => {
            if *bytes.get(20)? != 0x2F {
                return None;
            }
            let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
            let width = 1 + u64::from(bits & 0x3FFF);
            let height = 1 + u64::from((bits >> 14) & 0x3FFF);
            Some((width, height))
        }
        _ => None,
    }
}

fn read_le24(bytes: &[u8]) -> u64 {
    u64::from(bytes[0]) | (u64::from(bytes[1]) << 8) | (u64::from(bytes[2]) << 16)
}

#[cfg(test)]
mod tests {
    use super::{content_range_total, ensure_dy_q, parse_image_dimensions, HttpClient};

    #[test]
    fn appends_dy_q_when_missing() {
        let url = ensure_dy_q("https://example.com/video/".to_string());
        assert!(url.contains("dy_q="));
        assert!(url.ends_with("dy_va_biz_cert="));
    }

    #[test]
    fn keeps_existing_dy_q() {
        let url = "https://example.com/video/?dy_q=1&dy_va_biz_cert=".to_string();
        assert_eq!(ensure_dy_q(url.clone()), url);
    }

    #[test]
    fn parses_content_range_total() {
        assert_eq!(content_range_total("bytes 0-0/12345"), Some(12345));
        assert_eq!(content_range_total("bytes 0-0/*"), None);
        assert_eq!(content_range_total("bytes 0-0/0"), None);
        assert_eq!(content_range_total("garbage"), None);
    }

    #[test]
    fn parses_jpeg_dimensions() {
        let bytes = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x04,
            0x38, 0x03, 0x29, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
        ];
        assert_eq!(parse_image_dimensions(&bytes), Some((809, 1080)));
    }

    #[test]
    fn parses_png_dimensions() {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 13]);
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&1080u32.to_be_bytes());
        bytes.extend_from_slice(&1920u32.to_be_bytes());
        assert_eq!(parse_image_dimensions(&bytes), Some((1080, 1920)));
    }

    #[test]
    fn parses_webp_dimensions() {
        let mut vp8x = b"RIFF".to_vec();
        vp8x.extend_from_slice(&[0, 0, 0, 0]);
        vp8x.extend_from_slice(b"WEBPVP8X");
        vp8x.extend_from_slice(&[10, 0, 0, 0, 0, 0, 0, 0]);
        vp8x.extend_from_slice(&[0x28, 0x03, 0x00]);
        vp8x.extend_from_slice(&[0x37, 0x04, 0x00]);
        assert_eq!(parse_image_dimensions(&vp8x), Some((809, 1080)));

        let mut vp8l = b"RIFF".to_vec();
        vp8l.extend_from_slice(&[0, 0, 0, 0]);
        vp8l.extend_from_slice(b"WEBPVP8L");
        vp8l.extend_from_slice(&[10, 0, 0, 0, 0x2F]);
        let bits: u32 = (809u32 - 1) | ((1080u32 - 1) << 14);
        vp8l.extend_from_slice(&bits.to_le_bytes());
        assert_eq!(parse_image_dimensions(&vp8l), Some((809, 1080)));
    }

    #[test]
    fn head_meta_falls_back_to_range_get() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 1024];
                let read = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..read]);
                let response = if request.starts_with("HEAD ") {
                    "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                } else {
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/12345\r\nContent-Type: video/mp4\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx"
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let client = HttpClient::new().unwrap();
        let (length, content_type) = client.head_meta(&format!("http://{addr}/video.mp4"));
        assert_eq!(length, Some(12345));
        assert_eq!(content_type.as_deref(), Some("video/mp4"));
    }

    #[test]
    fn head_meta_batch_reports_every_url() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..3 {
                let Ok((mut stream, _)) = listener.accept() else {
                    continue;
                };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 54321\r\nContent-Type: image/jpeg\r\nConnection: close\r\n\r\n",
                );
            }
        });

        let client = HttpClient::new().unwrap();
        let urls = [
            format!("http://{addr}/a.jpg"),
            format!("http://{addr}/b.jpg"),
            format!("http://{addr}/c.jpg"),
        ];
        let refs: Vec<&str> = urls.iter().map(String::as_str).collect();
        let metas = client.head_meta_batch(&refs);
        assert_eq!(metas.len(), 3);
        for (length, content_type) in metas {
            assert_eq!(length, Some(54321));
            assert_eq!(content_type.as_deref(), Some("image/jpeg"));
        }
        server.join().unwrap();
    }
}
