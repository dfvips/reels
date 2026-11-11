//! HTML、重定向与播放地址请求。

use std::io::Read;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, LOCATION,
    RANGE, SET_COOKIE, USER_AGENT,
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
            let mut request = self
                .client
                .get(&current)
                .headers(headers.clone());
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
    pub fn head_meta(&self, url: &str) -> (Option<u64>, Option<String>) {
        let Ok(response) = self.client.head(url).headers(ios_headers()).send() else {
            return (None, None);
        };
        if !response.status().is_success() {
            return (None, None);
        }
        let length = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        (length, content_type)
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
            return Some(location);
        }
        let response = self.client.get(&url).headers(ios_headers()).send().ok()?;
        if matches!(response.status().as_u16(), 301 | 302) {
            return response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
        }
        Some(url)
    }
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
