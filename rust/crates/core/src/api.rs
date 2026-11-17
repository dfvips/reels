//! 抖音签名接口（aweme detail）。

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use md5::{Digest, Md5};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, COOKIE, REFERER, USER_AGENT};
use serde_json::{json, Value};

use crate::a_bogus::{ABogus, OsRandom};

/// 长期有效的访客 ID。
pub const UIFID: &str = "963b855e412d3196d1c3a882f7f19fd82d0f634397c778689a74b98535e82730a4f3989c9e0abbd48f6171b2f9d78a2e4c7c6db129bbdc9d3136b80e637bad2dee6112dd4ae9ed05d704818aba3519cee3b3d6c5d95a42d0c2d962c96fc1e857321e48c02a520ffaa0b74b363879a721";
pub const API_SALT: &str = "A96D855A08C0A9707F8BEF0D9A527E4E";
pub const API_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";

#[derive(Debug, Clone)]
pub struct AwemeDetail {
    pub aweme_id: String,
    pub desc: Option<String>,
    pub play_uri: Option<String>,
    pub play_urls: Vec<String>,
    pub bit_rates: Vec<AwemeBitRate>,
    pub duration_ms: Option<u64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub size_bytes: Option<u64>,
    pub quality: Option<String>,
    /// 图文 / 实况（图 + 视频）作品的图片列表。
    pub images: Vec<AwemeImage>,
    /// 图文作品的背景音乐。
    pub music: Option<AwemeMusic>,
}

#[derive(Debug, Clone, Default)]
pub struct AwemeImage {
    pub url: String,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub live_video: Option<AwemeLiveVideo>,
}

#[derive(Debug, Clone, Default)]
pub struct AwemeLiveVideo {
    pub play_uri: Option<String>,
    pub play_urls: Vec<String>,
    pub duration_ms: Option<u64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct AwemeMusic {
    pub url: String,
    /// `music.duration` 的单位是秒（与视频的毫秒不同）。
    pub duration_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AwemeBitRate {
    pub quality: Option<String>,
    pub url: Option<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub size_bytes: Option<u64>,
}

pub struct DouyinApi {
    client: Client,
    ttwid: Mutex<Option<String>>,
    user_agent: String,
}

impl DouyinApi {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            ttwid: Mutex::new(None),
            user_agent: API_USER_AGENT.to_string(),
        })
    }

    /// 获取单条视频详情；接口拒绝（风控 / 视频不存在）时返回 `None`，
    /// 由调用方回退到 HTML 流程。
    pub fn fetch_aweme_detail(&self, aweme_id: &str) -> Option<AwemeDetail> {
        let ttwid = self.ensure_ttwid();
        let verify_fp = generate_verify_fp(now_ms());
        let ms_token = generate_ms_token();

        let pairs: Vec<(String, String)> = vec![
            ("device_platform", "webapp"),
            ("aid", "6383"),
            ("channel", "channel_pc_web"),
            ("aweme_id", aweme_id),
            ("pc_client_type", "1"),
            ("version_code", "190500"),
            ("version_name", "19.5.0"),
            ("cookie_enabled", "true"),
            ("screen_width", "1920"),
            ("screen_height", "1080"),
            ("browser_language", "zh-CN"),
            ("browser_platform", "Win32"),
            ("browser_name", "Chrome"),
            ("browser_version", "146.0.0.0"),
            ("browser_online", "true"),
            ("engine_name", "Blink"),
            ("engine_version", "146.0.0.0"),
            ("os_name", "Windows"),
            ("os_version", "10"),
            ("cpu_core_num", "8"),
            ("device_memory", "8"),
            ("platform", "PC"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .chain(std::iter::once(("msToken".to_string(), ms_token)))
        .collect();

        let mut query = encode_query(&pairs);
        {
            let mut rng = OsRandom;
            let mut bogus = ABogus::new(self.user_agent.clone(), &mut rng);
            let value = bogus.get_value(&query, "", now_ms()).ok()?;
            query.push_str("&a_bogus=");
            query.push_str(&percent_encode(&value, ""));
        }

        let stamp = (now_ms() / 1000).to_string();
        let mut qpairs: Vec<(String, String)> = query
            .split('&')
            .filter(|part| !part.is_empty())
            .map(|part| match part.split_once('=') {
                Some((name, value)) => (percent_decode(name), percent_decode(value)),
                None => (percent_decode(part), String::new()),
            })
            .collect();
        qpairs.push(("verifyFp".to_string(), verify_fp.clone()));
        qpairs.push(("fp".to_string(), verify_fp.clone()));
        qpairs.push(("uifid".to_string(), UIFID.to_string()));
        qpairs.push(("timestamp".to_string(), stamp.clone()));
        let encoded = qpairs
            .iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    percent_encode(name, "*-._"),
                    percent_encode(value, "*-._")
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let signature = md5_hex(&format!("{UIFID}_{stamp}_{API_SALT}_{encoded}"));
        let final_query = format!("{encoded}&x-secsdk-web-signature={signature}");

        let url = format!("https://www.douyin.com/aweme/v1/web/aweme/detail/?{final_query}");
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent).ok()?);
        headers.insert(
            REFERER,
            HeaderValue::from_str(&format!("https://www.douyin.com/video/{aweme_id}")).ok()?,
        );
        headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!("ttwid={ttwid}; s_v_web_id={verify_fp}")).ok()?,
        );
        headers.insert("uifid", HeaderValue::from_str(UIFID).ok()?);
        headers.insert(
            "x-secsdk-web-signature",
            HeaderValue::from_str(&signature).ok()?,
        );
        headers.insert("x-secsdk-web-expire", HeaderValue::from_str(&stamp).ok()?);

        let response = self.client.get(&url).headers(headers).send().ok()?;
        if !response.status().is_success() {
            return None;
        }
        let data: Value = response.json().ok()?;
        if data.get("status_code").and_then(Value::as_i64) != Some(0) {
            return None;
        }
        let detail = data.get("aweme_detail")?;
        let desc = detail
            .get("desc")
            .and_then(Value::as_str)
            .map(str::to_string);
        // 图文 / 实况作品的 top-level video 只是幻灯片或背景音乐占位，
        // 真正的媒体在 images 里（实况图会额外带一段 video）。
        let images = parse_images(detail);
        if !images.is_empty() {
            return Some(AwemeDetail {
                aweme_id: aweme_id.to_string(),
                desc,
                play_uri: None,
                play_urls: Vec::new(),
                bit_rates: Vec::new(),
                duration_ms: None,
                width: None,
                height: None,
                size_bytes: None,
                quality: None,
                images,
                music: parse_music(detail),
            });
        }
        let play_addr = detail.get("video")?.get("play_addr")?;
        let urls: Vec<String> = play_addr
            .get("url_list")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(Value::as_str)
            .filter(|url| !url.is_empty())
            .map(str::to_string)
            .collect();
        if urls.is_empty() {
            return None;
        }
        let video = detail.get("video")?;
        let duration_ms = video.get("duration").and_then(Value::as_u64);
        let width = video.get("width").and_then(Value::as_u64);
        let height = video.get("height").and_then(Value::as_u64);
        let play_uri = play_addr.get("uri").and_then(Value::as_str);
        let bit_rates = video
            .get("bit_rate")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let play_addr = item.get("play_addr");
                        let url = play_addr
                            .and_then(|addr| addr.get("url_list"))
                            .and_then(Value::as_array)?
                            .iter()
                            .filter_map(Value::as_str)
                            .find(|url| !url.is_empty())?
                            .to_string();
                        Some(AwemeBitRate {
                            quality: item
                                .get("gear_name")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            url: Some(url),
                            width: item.get("width").and_then(Value::as_u64).or_else(|| {
                                play_addr
                                    .and_then(|addr| addr.get("width"))
                                    .and_then(Value::as_u64)
                            }),
                            height: item.get("height").and_then(Value::as_u64).or_else(|| {
                                play_addr
                                    .and_then(|addr| addr.get("height"))
                                    .and_then(Value::as_u64)
                            }),
                            size_bytes: item.get("size").and_then(Value::as_u64).or_else(|| {
                                play_addr
                                    .and_then(|addr| addr.get("data_size"))
                                    .and_then(Value::as_u64)
                            }),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut size_bytes = None;
        let mut quality = video
            .get("video_quality")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(bit_rates) = video.get("bit_rate").and_then(Value::as_array) {
            let matched = bit_rates
                .iter()
                .find(|item| {
                    item.get("play_addr")
                        .and_then(|addr| addr.get("uri"))
                        .and_then(Value::as_str)
                        .zip(play_uri)
                        .is_some_and(|(uri, play)| uri == play)
                })
                .or_else(|| bit_rates.first());
            if let Some(item) = matched {
                size_bytes = item.get("size").and_then(Value::as_u64);
                if let Some(gear) = item.get("gear_name").and_then(Value::as_str) {
                    quality = Some(gear.to_string());
                }
            }
        }
        Some(AwemeDetail {
            aweme_id: aweme_id.to_string(),
            desc,
            play_uri: play_uri.map(str::to_string),
            play_urls: urls,
            bit_rates,
            duration_ms,
            width,
            height,
            size_bytes,
            quality,
            images: Vec::new(),
            music: None,
        })
    }

    fn ensure_ttwid(&self) -> String {
        if let Some(cached) = self.ttwid.lock().ok().and_then(|guard| guard.clone()) {
            return cached;
        }
        let body = json!({
            "region": "cn",
            "aid": 1768,
            "needFid": false,
            "service": "www.ixigua.com",
            "migrate_info": {"ticket": "", "source": "node"},
            "cbUrlProtocol": "https",
            "union": true
        });
        let response = self
            .client
            .post("https://ttwid.bytedance.com/ttwid/union/register/")
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send();
        let mut token = String::new();
        if let Ok(response) = response {
            for value in response.headers().get_all(reqwest::header::SET_COOKIE) {
                if let Ok(text) = value.to_str() {
                    if let Some(rest) = text.strip_prefix("ttwid=") {
                        token = rest.split(';').next().unwrap_or("").to_string();
                        break;
                    }
                }
            }
        }
        if !token.is_empty() {
            if let Ok(mut guard) = self.ttwid.lock() {
                *guard = Some(token.clone());
            }
        }
        token
    }
}

fn parse_images(detail: &Value) -> Vec<AwemeImage> {
    detail
        .get("images")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_image).collect())
        .unwrap_or_default()
}

fn parse_image(image: &Value) -> Option<AwemeImage> {
    let url = image_url(image)?;
    let live_video = image.get("video").and_then(parse_live_video);
    Some(AwemeImage {
        url,
        width: image.get("width").and_then(Value::as_u64),
        height: image.get("height").and_then(Value::as_u64),
        live_video,
    })
}

/// `url_list` 同时给出 webp 与 jpeg 的压缩预览地址，优先 jpeg 以便
/// 本地查看器和相册直接识别；签名绑定完整路径，不能改成原图地址。
fn image_url(image: &Value) -> Option<String> {
    let urls: Vec<&str> = image
        .get("url_list")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .filter(|url| !url.is_empty())
        .collect();
    urls.iter()
        .find(|url| {
            let path = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
            path.ends_with(".jpeg") || path.ends_with(".jpg")
        })
        .or_else(|| urls.first())
        .map(|url| (*url).to_string())
}

fn parse_live_video(video: &Value) -> Option<AwemeLiveVideo> {
    let play_addr = video.get("play_addr");
    let play_uri = play_addr
        .and_then(|addr| addr.get("uri"))
        .and_then(Value::as_str)
        .filter(|uri| !uri.is_empty())
        .map(str::to_string);
    let play_urls: Vec<String> = play_addr
        .and_then(|addr| addr.get("url_list"))
        .and_then(Value::as_array)
        .map(|urls| {
            urls.iter()
                .filter_map(Value::as_str)
                .filter(|url| !url.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if play_uri.is_none() && play_urls.is_empty() {
        return None;
    }
    Some(AwemeLiveVideo {
        play_uri,
        play_urls,
        duration_ms: video.get("duration").and_then(Value::as_u64),
        width: play_addr
            .and_then(|addr| addr.get("width"))
            .and_then(Value::as_u64),
        height: play_addr
            .and_then(|addr| addr.get("height"))
            .and_then(Value::as_u64),
        size_bytes: play_addr
            .and_then(|addr| addr.get("data_size"))
            .and_then(Value::as_u64),
    })
}

/// `music.play_url.url_list` 是背景音乐 mp3 的直链；缺少地址时返回 None。
fn parse_music(detail: &Value) -> Option<AwemeMusic> {
    let music = detail.get("music")?;
    let url = music
        .get("play_url")
        .and_then(|play_url| play_url.get("url_list"))
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .find(|url| url.starts_with("http"))?
        .to_string();
    Some(AwemeMusic {
        url,
        duration_secs: music
            .get("duration")
            .and_then(Value::as_u64)
            .filter(|secs| *secs > 0),
    })
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn generate_verify_fp(now: i64) -> String {
    use rand::Rng as _;
    let mut tail: Vec<char> = vec!['\0'; 36];
    for index in [8, 13, 18, 23] {
        tail[index] = '_';
    }
    tail[14] = '4';
    let alphabet: Vec<char> = "0123456789abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut rng = rand::rng();
    for (i, slot) in tail.iter_mut().enumerate() {
        if *slot != '\0' {
            continue;
        }
        let mut position = rng.random_range(0..alphabet.len());
        if i == 19 {
            position = (3 & position) | 8;
        }
        *slot = alphabet[position];
    }
    format!(
        "verify_{}_{}",
        to_base36(now),
        tail.into_iter().collect::<String>()
    )
}

fn generate_ms_token() -> String {
    use rand::Rng as _;
    let alphabet: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
        .chars()
        .collect();
    let mut rng = rand::rng();
    let body: String = (0..116)
        .map(|_| alphabet[rng.random_range(0..alphabet.len())])
        .collect();
    format!("{body}==")
}

fn to_base36(value: i64) -> String {
    if value <= 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    let mut v = value;
    while v > 0 {
        let remainder = (v % 36) as u32;
        digits.push(if remainder < 10 {
            char::from(b'0' + remainder as u8)
        } else {
            char::from(b'a' + (remainder - 10) as u8)
        });
        v /= 36;
    }
    digits.into_iter().rev().collect()
}

/// Python `urllib.parse.urlencode` 等价的查询串编码。
pub fn encode_query(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", percent_encode(name, ""), percent_encode(value, "")))
        .collect::<Vec<_>>()
        .join("&")
}

/// Python `urllib.parse.quote(value, safe=safe_extra)` 等价实现。
pub fn percent_encode(value: &str, safe_extra: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        let ch = *byte as char;
        if is_unreserved(*byte) || safe_extra.contains(ch) {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

pub fn md5_hex(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_encoding_matches_python() {
        assert_eq!(
            percent_encode("a b&c=d/测试", ""),
            "a%20b%26c%3Dd%2F%E6%B5%8B%E8%AF%95"
        );
        assert_eq!(percent_decode("a%20b%26c"), "a b&c");
        assert_eq!(percent_encode("keep*-._ok", "*-._"), "keep*-._ok");
    }

    #[test]
    fn base36_and_md5() {
        assert_eq!(to_base36(1_763_000_000_000), "mhwsoi68");
        assert_eq!(md5_hex("abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn parses_music_play_url() {
        let detail = json!({
            "music": {
                "duration": 32,
                "play_url": {
                    "uri": "https://sf11.douyinstatic.com/obj/ies-music/bgm.mp3",
                    "url_list": [
                        "https://sf11.douyinstatic.com/obj/ies-music/bgm.mp3"
                    ]
                }
            }
        });
        let music = parse_music(&detail).unwrap();
        assert!(music.url.ends_with("bgm.mp3"));
        assert_eq!(music.duration_secs, Some(32));

        let zero_duration = json!({
            "music": {
                "duration": 0,
                "play_url": { "url_list": ["https://sf11.douyinstatic.com/obj/ies-music/bgm.mp3"] }
            }
        });
        assert_eq!(parse_music(&zero_duration).unwrap().duration_secs, None);
    }

    #[test]
    fn parses_image_and_live_video() {
        let detail = json!({
            "images": [
                {
                    "url_list": [
                        "https://p3.douyinpic.com/a~tplv-dy-aweme-images:q75.webp?x=1",
                        "https://p3.douyinpic.com/a~tplv-dy-aweme-images:q75.jpeg?x=1"
                    ],
                    "width": 2160,
                    "height": 2880,
                    "video": {
                        "duration": 2809,
                        "play_addr": {
                            "uri": "v0200fg10000abc",
                            "width": 720,
                            "height": 960,
                            "data_size": 571289,
                            "url_list": ["https://v95.douyinvod.com/live.mp4"]
                        }
                    }
                },
                {
                    "url_list": ["https://p3.douyinpic.com/b~tplv-dy-aweme-images:q75.webp?x=1"],
                    "width": 720,
                    "height": 958
                }
            ]
        });
        let images = parse_images(&detail);
        assert_eq!(images.len(), 2);
        assert!(images[0].url.ends_with(".jpeg?x=1"));
        assert_eq!(images[0].width, Some(2160));
        let live = images[0].live_video.as_ref().unwrap();
        assert_eq!(live.play_uri.as_deref(), Some("v0200fg10000abc"));
        assert_eq!(live.duration_ms, Some(2809));
        assert_eq!(live.size_bytes, Some(571289));
        assert!(images[1].live_video.is_none());
    }
}
