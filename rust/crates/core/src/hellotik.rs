//! hellotik.app RAW 解析协议（xhs_app 内置的小红书高清解析服务）。
//!
//! 流程与网页端完全一致：
//! 1. geoip 取 isoCode/IP，generate-id 换取服务端签发的 `uwx_id`；
//! 2. `POST /api/gate-e5eea8` 换取 ticket（2 分钟有效期）；
//! 3. 参数 JSON 以 AES-256-GCM（key = SHA256(tk:sd)，12 字节 nonce）加密，
//!    密文（含 tag）放 `pl_e5eea8`，nonce 单独放 `iv_e5eea8`，连同 ticket 以
//!    `{tk_e5eea8, pl_e5eea8, iv_e5eea8, vr_e5eea8}` 信封提交 `POST /api/parse`；
//! 4. 响应体先经 XOR 90 / 8 字符块反转 / 自定义字母表重映射，再以
//!    AES-256-CBC（固定 key）解密出 JSON。

use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes256;
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::Rng;
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, CONTENT_TYPE, ORIGIN, PRAGMA,
    REFERER,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

const GATE_URL: &str = "https://www.hellotik.app/api/gate-e5eea8";
const PARSE_URL: &str = "https://www.hellotik.app/api/parse";
const UWX_URL: &str = "https://www.hellotik.app/account/user/api/uwx/generate-id";
const GEOIP_URL: &str = "https://user.hellotik.app/getip/geoip?minimal=1";
const TICKET_FIELD: &str = "tk_e5eea8";
const SEED_FIELD: &str = "sd_e5eea8";
const PAYLOAD_FIELD: &str = "pl_e5eea8";
const IV_FIELD: &str = "iv_e5eea8";
const VERSION_FIELD: &str = "vr_e5eea8";
const PARSE_VERSION: u32 = 1;
const RESPONSE_SALT: &str = "93838338562359368888868323563256";

const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
const SEC_CH_UA: &str =
    "\"Chromium\";v=\"146\", \"Not-A.Brand\";v=\"24\", \"Google Chrome\";v=\"146\"";

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("hellotik http client")
});

const CUSTOM_ALPHABET: &[u8; 64] =
    b"ZYXABCDEFGHIJKLMNOPQRSTUVWzyxabcdefghijklmnopqrstuvw9876543210-_";
const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, Default)]
pub struct HellotikMedia {
    pub title: String,
    pub url: String,
    pub extension: Option<String>,
}

fn request_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://www.hellotik.app"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://www.hellotik.app/zh/rednote"),
    );
    headers.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
    headers.insert("sec-ch-ua", HeaderValue::from_static(SEC_CH_UA));
    headers.insert("sec-ch-ua-mobile", HeaderValue::from_static("?0"));
    headers.insert("sec-ch-ua-platform", HeaderValue::from_static("\"macOS\""));
    headers.insert("sec-fetch-dest", HeaderValue::from_static("empty"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("cors"));
    headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
    headers
}

fn post_headers() -> HeaderMap {
    let mut headers = request_headers();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers
}

/// 解析一条小红书链接，返回 RAW 清晰度直链。
pub fn parse_video(url: &str) -> Result<HellotikMedia, String> {
    let (iso_code, geoip_ip) = fetch_geoip();
    let uwx_id = request_uwx_id();
    let (ticket, seed) = request_ticket(url)?;
    let body = build_envelope(url, &uwx_id, &iso_code, &geoip_ip, &ticket, &seed)?;
    let response = CLIENT
        .post(PARSE_URL)
        .headers(request_headers())
        .body(body)
        .send()
        .map_err(|_| "hellotik 网络请求异常".to_string())?;
    let status = response.status().as_u16();
    let json: Value = response
        .json()
        .map_err(|_| "hellotik 响应解析异常".to_string())?;
    if status != 200 || json.get("status").and_then(Value::as_i64) != Some(0) {
        return Err(format_parse_error(&json));
    }
    let payload = if json.get("encrypt").and_then(Value::as_bool) == Some(true) {
        let data = json
            .get("data")
            .and_then(Value::as_str)
            .ok_or("hellotik 响应缺少 data")?;
        let key = json
            .get("key")
            .and_then(Value::as_str)
            .ok_or("hellotik 响应缺少 key")?;
        decrypt_response(data, key)?
    } else {
        json.get("data").cloned().ok_or("hellotik 响应缺少 data")?
    };
    extract_media(&payload)
}

fn format_parse_error(json: &Value) -> String {
    let reason = json
        .get("reason")
        .or_else(|| json.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if reason == "ticket_ip_restricted" {
        return "解析服务暂时限流，请稍后重试".to_string();
    }
    format!("解析失败（{reason}）")
}

fn request_ticket(url: &str) -> Result<(String, String), String> {
    let body = serde_json::json!({
        "requestURL": url,
        "isBatch": false,
        "mode": "single",
    });
    let response = CLIENT
        .post(GATE_URL)
        .headers(post_headers())
        .body(body.to_string())
        .send()
        .map_err(|_| "hellotik 网络请求异常".to_string())?;
    let status = response.status().as_u16();
    let json: Value = response
        .json()
        .map_err(|_| "hellotik 响应解析异常".to_string())?;
    let ticket = json.get(TICKET_FIELD).and_then(Value::as_str);
    let seed = json.get(SEED_FIELD).and_then(Value::as_str);
    match (status == 200, ticket, seed) {
        (true, Some(ticket), Some(seed)) => Ok((ticket.to_string(), seed.to_string())),
        _ => Err(format_parse_error(&json)),
    }
}

fn fetch_geoip() -> (String, String) {
    let fallback = ("Other".to_string(), String::new());
    let response = match CLIENT.get(GEOIP_URL).headers(request_headers()).send() {
        Ok(response) => response,
        Err(_) => return fallback,
    };
    if response.status().as_u16() != 200 {
        return fallback;
    }
    let json: Value = match response.json() {
        Ok(json) => json,
        Err(_) => return fallback,
    };
    let data = json.get("data");
    let iso_code = data
        .and_then(|data| data.get("isoCode"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("Other");
    let ip = data
        .and_then(|data| data.get("ip"))
        .and_then(Value::as_str)
        .unwrap_or("");
    (iso_code.to_string(), ip.to_string())
}

fn request_uwx_id() -> String {
    let response = match CLIENT
        .post(UWX_URL)
        .headers(post_headers())
        .body("{}")
        .send()
    {
        Ok(response) => response,
        Err(_) => return generate_uwx_id(),
    };
    if response.status().as_u16() != 200 {
        return generate_uwx_id();
    }
    let json: Value = match response.json() {
        Ok(json) => json,
        Err(_) => return generate_uwx_id(),
    };
    json.get("data")
        .and_then(|data| data.get("uwx_id"))
        .and_then(Value::as_str)
        .filter(|value| value.starts_with("uwx"))
        .map(str::to_string)
        .unwrap_or_else(generate_uwx_id)
}

/// 与网页端降级逻辑一致的本地 uwx_id：uwx_<6位时间尾数><4位base36><2位大写>。
fn generate_uwx_id() -> String {
    let mut rng = rand::rng();
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tail = format!("{:0>6}", millis % 1_000_000);
    let base36: String = (0..4)
        .map(|_| {
            let index = rng.random_range(0..36);
            char::from_digit(index as u32, 36).unwrap_or('0')
        })
        .collect();
    let letters: String = (0..2)
        .map(|_| (b'A' + rng.random_range(0..26u8)) as char)
        .collect();
    format!("uwx_{tail}{base36}{letters}")
}

fn build_params_json(url: &str, uwx_id: &str, iso_code: &str, geoip_ip: &str) -> String {
    let request_url = Value::String(url.to_string()).to_string();
    let uwx = Value::String(uwx_id.to_string()).to_string();
    let iso = Value::String(iso_code.to_string()).to_string();
    let ip = Value::String(geoip_ip.to_string()).to_string();
    format!(
        concat!(
            "{{\"requestURL\":{request_url},\"isMobile\":\"false\",\"isoCode\":{iso},",
            "\"adType\":\"adsense\",\"uwx_id\":{uwx},\"successCount\":\"0\",",
            "\"totalSuccessCount\":\"0\",\"firstSuccessDate\":null,\"geoipIp\":{ip}}}"
        ),
        request_url = request_url,
        iso = iso,
        ip = ip,
        uwx = uwx,
    )
}

fn build_envelope(
    url: &str,
    uwx_id: &str,
    iso_code: &str,
    geoip_ip: &str,
    ticket: &str,
    seed: &str,
) -> Result<String, String> {
    let plain = build_params_json(url, uwx_id, iso_code, geoip_ip);
    let key = Sha256::digest(format!("{ticket}:{seed}").as_bytes());
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "加密初始化失败".to_string())?;
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plain.as_bytes())
        .map_err(|_| "参数加密失败".to_string())?;
    let payload_b64 = BASE64.encode(&ciphertext);
    let iv_b64 = BASE64.encode(nonce_bytes);
    let ticket_json = Value::String(ticket.to_string()).to_string();
    Ok(format!(
        "{{\"{TICKET_FIELD}\":{ticket_json},\"{PAYLOAD_FIELD}\":\"{payload_b64}\",\"{IV_FIELD}\":\"{iv_b64}\",\"{VERSION_FIELD}\":{PARSE_VERSION}}}"
    ))
}

/// 响应变换：XOR 90 → 8 字符块反转 → 字母表重映射 → 标准 base64。
fn decode_transform(raw: &[u8]) -> Result<Vec<u8>, String> {
    let xored: Vec<u8> = raw.iter().map(|byte| byte ^ 90).collect();
    let reversed = block_reverse(&xored);
    let remapped: Vec<u8> = reversed
        .iter()
        .map(
            |byte| match CUSTOM_ALPHABET.iter().position(|c| c == byte) {
                Some(index) => STANDARD_ALPHABET[index],
                None => *byte,
            },
        )
        .collect();
    BASE64
        .decode(&remapped)
        .map_err(|_| "hellotik 响应变换失败".to_string())
}

fn block_reverse(bytes: &[u8]) -> Vec<u8> {
    bytes
        .chunks(8)
        .flat_map(|chunk| chunk.iter().rev())
        .copied()
        .collect()
}

fn decrypt_response(data_b64: &str, key_b64: &str) -> Result<Value, String> {
    let data = BASE64
        .decode(data_b64)
        .map_err(|_| "hellotik 响应 data 解码失败".to_string())
        .and_then(|raw| decode_transform(&raw))?;
    let iv = BASE64
        .decode(key_b64)
        .map_err(|_| "hellotik 响应 key 解码失败".to_string())
        .and_then(|raw| decode_transform(&raw))?;
    let plaintext = aes_cbc_decrypt(RESPONSE_SALT.as_bytes(), &iv, &data)?;
    let text = String::from_utf8(plaintext).map_err(|_| "hellotik 响应编码异常".to_string())?;
    serde_json::from_str(&text).map_err(|_| "hellotik 响应 JSON 解析失败".to_string())
}

fn aes_cbc_decrypt(key: &[u8], iv: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if key.len() != 32 || iv.len() != 16 || ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return Err("hellotik AES-CBC 参数无效".to_string());
    }
    let cipher =
        Aes256::new_from_slice(key).map_err(|_| "hellotik AES-CBC 初始化失败".to_string())?;
    let mut plaintext = Vec::with_capacity(ciphertext.len());
    let mut previous = iv.to_vec();
    for block in ciphertext.chunks(16) {
        let mut decrypted = [0u8; 16];
        decrypted.copy_from_slice(block);
        cipher.decrypt_block(&mut decrypted.into());
        for (index, byte) in decrypted.iter_mut().enumerate() {
            *byte ^= previous[index];
        }
        plaintext.extend_from_slice(&decrypted);
        previous = block.to_vec();
    }
    let padding = *plaintext.last().ok_or("hellotik AES-CBC 空明文")? as usize;
    if padding == 0 || padding > 16 || padding > plaintext.len() {
        return Err("hellotik AES-CBC 填充无效".to_string());
    }
    plaintext.truncate(plaintext.len() - padding);
    Ok(plaintext)
}

fn extract_media(payload: &Value) -> Result<HellotikMedia, String> {
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let medias = payload
        .get("medias")
        .and_then(Value::as_array)
        .ok_or("hellotik 响应缺少 medias")?;
    // 优先 RAW 清晰度，其次取体积最大的视频项。
    let mut best: Option<&Value> = None;
    let mut best_score: (u8, u64) = (0, 0);
    for item in medias {
        if item.get("type").and_then(Value::as_str) != Some("video") {
            continue;
        }
        let url = item
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| !url.is_empty());
        if url.is_none() {
            continue;
        }
        let quality_raw = item.get("quality").and_then(Value::as_str).unwrap_or("");
        let is_raw = quality_raw.eq_ignore_ascii_case("raw") as u8;
        let size = item
            .get("size")
            .or_else(|| item.get("bytes"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let score = (is_raw, size);
        if best.map(|_| score > best_score).unwrap_or(true) {
            best = Some(item);
            best_score = score;
        }
    }
    let item = best.ok_or("hellotik 未返回可用视频")?;
    let url = item
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let extension = item
        .get("extension")
        .and_then(Value::as_str)
        .map(|value| value.trim_start_matches('.').to_string());
    Ok(HellotikMedia {
        title,
        url,
        extension: extension.or(Some("mp4".to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 `decode_transform` 对应的编码方向（模拟服务端）：
    /// 标准 base64 → 逆字母表重映射 → 块反转 → XOR 90 → 标准 base64。
    fn encode_transform(raw: &[u8]) -> String {
        let b64 = BASE64.encode(raw);
        let remapped: Vec<u8> = b64
            .bytes()
            .map(
                |byte| match STANDARD_ALPHABET.iter().position(|c| *c == byte) {
                    Some(index) => CUSTOM_ALPHABET[index],
                    None => byte,
                },
            )
            .collect();
        let reversed = block_reverse(&remapped);
        let xored: Vec<u8> = reversed.iter().map(|byte| byte ^ 90).collect();
        BASE64.encode(&xored)
    }

    fn aes_cbc_encrypt(key: &[u8], iv: &[u8], plaintext: &[u8]) -> Vec<u8> {
        use aes::cipher::BlockEncrypt;
        let cipher = Aes256::new_from_slice(key).unwrap();
        let padding = 16 - (plaintext.len() % 16);
        let mut padded = plaintext.to_vec();
        padded.extend(std::iter::repeat(padding as u8).take(padding));
        let mut previous = iv.to_vec();
        let mut ciphertext = Vec::new();
        for block in padded.chunks(16) {
            let mut encrypted = [0u8; 16];
            encrypted.copy_from_slice(block);
            for (index, byte) in encrypted.iter_mut().enumerate() {
                *byte ^= previous[index];
            }
            cipher.encrypt_block(&mut encrypted.into());
            ciphertext.extend_from_slice(&encrypted);
            previous = encrypted.to_vec();
        }
        ciphertext
    }

    #[test]
    fn response_decrypt_roundtrip() {
        let iv = [7u8; 16];
        let plaintext = r#"{"title":"战歌+1","medias":[{"type":"video","quality":"RAW","url":"https://sns-video-v2.xhscdn.com/stream/x.mp4","size":2089594982}]}"#.as_bytes();
        let ciphertext = aes_cbc_encrypt(RESPONSE_SALT.as_bytes(), &iv, plaintext);
        let data_b64 = encode_transform(&ciphertext);
        let key_b64 = encode_transform(&iv);
        let payload = decrypt_response(&data_b64, &key_b64).unwrap();
        assert_eq!(payload["title"], "战歌+1");
        assert_eq!(payload["medias"][0]["quality"], "RAW");
    }

    #[test]
    fn uwx_id_matches_web_fallback_shape() {
        let id = generate_uwx_id();
        assert!(id.starts_with("uwx_"));
        let rest = &id["uwx_".len()..];
        assert_eq!(rest.len(), 12);
        assert!(rest.chars().take(6).all(|c| c.is_ascii_digit()));
        assert!(rest[6..10]
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        assert!(rest[10..].chars().all(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn params_json_keeps_site_key_order() {
        let json = build_params_json(
            "https://www.xiaohongshu.com/x?token=AB=&src=1",
            "uwx_123456abcdXY",
            "CN",
            "1.2.3.4",
        );
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value["requestURL"],
            "https://www.xiaohongshu.com/x?token=AB=&src=1"
        );
        assert_eq!(value["isMobile"], "false");
        assert_eq!(value["isoCode"], "CN");
        assert_eq!(value["geoipIp"], "1.2.3.4");
        let expected_order = [
            "\"requestURL\":",
            "\"isMobile\":",
            "\"isoCode\":",
            "\"adType\":",
            "\"uwx_id\":",
            "\"successCount\":",
            "\"totalSuccessCount\":",
            "\"firstSuccessDate\":",
            "\"geoipIp\":",
        ];
        let mut last = 0usize;
        for marker in expected_order {
            let position = json[last..]
                .find(marker)
                .unwrap_or_else(|| panic!("missing {marker}"));
            last += position;
        }
    }

    #[test]
    fn envelope_gcm_roundtrip() {
        let envelope = build_envelope(
            "https://www.xiaohongshu.com/x",
            "uwx_123456abcdXY",
            "Other",
            "",
            "TK",
            "SD",
        )
        .unwrap();
        let value: Value = serde_json::from_str(&envelope).unwrap();
        assert_eq!(value["tk_e5eea8"], "TK");
        assert_eq!(value["vr_e5eea8"], 1);
        let iv = BASE64.decode(value["iv_e5eea8"].as_str().unwrap()).unwrap();
        let payload = BASE64.decode(value["pl_e5eea8"].as_str().unwrap()).unwrap();
        let key = Sha256::digest(b"TK:SD");
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let plain = cipher
            .decrypt(Nonce::from_slice(&iv), payload.as_slice())
            .unwrap();
        let params: Value = serde_json::from_slice(&plain).unwrap();
        assert_eq!(params["uwx_id"], "uwx_123456abcdXY");
    }

    #[test]
    fn extract_media_prefers_raw_then_largest() {
        let payload: Value = serde_json::json!({
            "title": "标题",
            "medias": [
                {"type": "image", "url": "https://img/a.jpg"},
                {"type": "video", "quality": "hd", "url": "https://v/small.mp4", "size": 100},
                {"type": "video", "quality": "RAW", "url": "https://v/raw.mp4", "size": 200},
                {"type": "video", "quality": "fhd", "url": "https://v/big.mp4", "size": 300}
            ]
        });
        let media = extract_media(&payload).unwrap();
        assert_eq!(media.url, "https://v/raw.mp4");
        assert_eq!(media.extension.as_deref(), Some("mp4"));
    }

    #[test]
    fn extract_media_without_raw_takes_largest_video() {
        let payload: Value = serde_json::json!({
            "title": "标题",
            "medias": [
                {"type": "video", "quality": "hd", "url": "https://v/small.mp4", "size": 100},
                {"type": "video", "quality": "fhd", "url": "https://v/big.mp4", "size": 300}
            ]
        });
        let media = extract_media(&payload).unwrap();
        assert_eq!(media.url, "https://v/big.mp4");
    }
}
