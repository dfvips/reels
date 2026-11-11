//! A-Bogus 签名实现。
//!
//! 随机噪声层与真实 SDK 一致：签名结果本身随随机数变化，但校验位
//! 只覆盖固定字段。测试通过固定随机序列与参考实现逐字节比对。

use std::collections::HashMap;
use std::fmt;

use crate::sm3::sm3_hash_str;

/// 随机源抽象：返回 `[0, max)` 的整数。
pub trait RandomSource {
    fn next_int(&mut self, max: u32) -> u32;
}

/// 默认随机源（线程随机数）。
pub struct OsRandom;

impl RandomSource for OsRandom {
    fn next_int(&mut self, max: u32) -> u32 {
        use rand::Rng as _;
        rand::rng().random_range(0..max.max(1))
    }
}

pub const SALT: &str = "dhzx";
pub const HEADER_MAGIC: [u8; 2] = [3, 82];
pub const SDK_VERSION: [u8; 4] = [1, 0, 1, 0];
pub const PAYLOAD_KEY: u8 = 0xD3;
pub const FORTNIGHT_EPOCH_MS: i64 = 1_721_836_800_000;
pub const PAGE_ID: u32 = 6241;
pub const AID: u32 = 6383;
pub const NOISE_MASKS: [u8; 3] = [0x91, 0x42, 0x2C];
pub const DATA_MASKS: [u8; 3] = [0x6E, 0xBD, 0xD3];
pub const ENV_FLAGS: u32 = 1;
pub const DETECT_FLAGS: u32 = 14;
pub const NR_FLAGS: u32 = 0x21;
pub const TRIPWIRE_LOCKED: u8 = 3;
pub const CALL_BUCKET: u8 = 6;
pub const DEFAULT_BROWSER_INFO: &str = "1920|947|1920|1032|1920|1032|1920|1080|Win32";

const FIELD_ORDER: [&str; 50] = [
    "L34", "L44", "L56", "L61", "L73", "L29", "L70", "L45", "L35", "L49", "L38", "L66", "L51",
    "L68", "L28", "L48", "L64", "L47", "L30", "L71", "L26", "L55", "L31", "L69", "L59", "L40",
    "L62", "L63", "L27", "L72", "L41", "L74", "L57", "L52", "L42", "L39", "L33", "L67", "L53",
    "L43", "L65", "L46", "L36", "L24", "L60", "L32", "L79", "L80", "L84", "L85",
];

const ENV_TAG: [u8; 4] = [0, 0, 0, 0];
const TRIPWIRE_SET: u8 = 0xB2;
const TRIPWIRE_FREE: u8 = 0x4D;

const ALPHABET_S3: &[u8] = b"ckdp1h4ZKsUB80/Mfvw36XIgR25+WQAlEi7NLboqYTOPuzmFjJnryx9HVGDaStCe";
const ALPHABET_S4: &[u8] = b"Dkdpgh2ZmsQB80/MfvV36XI1R45-WUAlEixNLwoqYTOPuzKFjJnry79HbGcaStCe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ABogusError {
    ClockBeforeEpoch(i64),
}

impl fmt::Display for ABogusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ABogusError::ClockBeforeEpoch(now) => {
                write!(f, "clock is before the a_bogus epoch: {now}")
            }
        }
    }
}

impl std::error::Error for ABogusError {}

/// 签名器。`rng` 用 `&mut dyn RandomSource` 注入，便于测试中回放固定序列。
pub struct ABogus<'a> {
    pub user_agent: String,
    pub browser_info: String,
    rng: &'a mut dyn RandomSource,
}

impl<'a> ABogus<'a> {
    pub fn new(user_agent: impl Into<String>, rng: &'a mut dyn RandomSource) -> Self {
        Self {
            user_agent: user_agent.into(),
            browser_info: DEFAULT_BROWSER_INFO.to_string(),
            rng,
        }
    }

    pub fn with_browser_info(mut self, browser_info: impl Into<String>) -> Self {
        self.browser_info = browser_info.into();
        self
    }

    /// 计算查询参数签名。
    pub fn get_value(
        &mut self,
        query: &str,
        body: &str,
        now_ms: i64,
    ) -> Result<String, ABogusError> {
        if now_ms < FORTNIGHT_EPOCH_MS {
            return Err(ABogusError::ClockBeforeEpoch(now_ms));
        }
        let fields = self.fields(query, body, now_ms);

        let mut version = Vec::with_capacity(8);
        version.extend(mask_pair(
            self.rng,
            [SDK_VERSION[0], SDK_VERSION[1]],
            None,
            None,
        ));
        // 参数求值顺序：low/high 先求出，再进入 mask_pair 抽噪声。
        let probe = probe_noise(self.rng);
        let tripwire = tripwire_noise(self.rng);
        version.extend(mask_pair(
            self.rng,
            [SDK_VERSION[2], SDK_VERSION[3]],
            Some(probe),
            Some(tripwire),
        ));

        let mut checksum: u64 = 0;
        for byte in &version {
            checksum ^= u64::from(*byte);
        }
        for name in FIELD_ORDER {
            checksum ^= u64::from(fields[name]);
        }

        let mut body_bytes = Vec::with_capacity(128);
        for name in FIELD_ORDER {
            body_bytes.push(fields[name]);
        }
        body_bytes.extend(js_bytes(&self.browser_info));
        body_bytes.extend(js_bytes(&format!("{},", (now_ms + 3) & 0xFF)));
        body_bytes.push((checksum & 0xFF) as u8);

        let header_noise = header_noise(self.rng, &self.user_agent);
        let header = mask_pair(self.rng, HEADER_MAGIC, None, Some(header_noise));
        let frame = expand_noise(self.rng, &body_bytes);

        let mut payload = Vec::with_capacity(version.len() + frame.len());
        payload.extend_from_slice(&version);
        payload.extend_from_slice(&frame);
        let sealed = rc4(&[PAYLOAD_KEY], &payload);

        let mut out = Vec::with_capacity(header.len() + sealed.len());
        out.extend_from_slice(&header);
        out.extend_from_slice(&sealed);
        Ok(encode_base64(&out, ALPHABET_S4))
    }

    fn fields(&mut self, query: &str, body: &str, now_ms: i64) -> HashMap<&'static str, u8> {
        let query_digest = digest_of(query);
        let body_digest = digest_of(body);
        let ua_digest = user_agent_digest(&self.user_agent);

        let ink = now_ms - 1;
        let fortnights = (now_ms - FORTNIGHT_EPOCH_MS) / (1000 * 60 * 60 * 24 * 14);
        let info_bytes = js_bytes(&self.browser_info);
        let tail_bytes = js_bytes(&format!("{},", (now_ms + 3) & 0xFF));

        let mut fields: HashMap<&'static str, u8> = HashMap::new();
        fields.insert("L24", 41);
        fields.insert("L26", fortnights as u8);
        fields.insert("L27", CALL_BUCKET);
        fields.insert("L28", 3);
        fields.insert("L35", (ENV_FLAGS & 0xFF) as u8);
        fields.insert("L36", ((ENV_FLAGS / 256) & 0xFF) as u8);
        fields.insert("L38", (NR_FLAGS & 0xFF) as u8);
        fields.insert("L39", ((NR_FLAGS >> 8) & 0xFF) as u8);
        fields.insert("L66", TRIPWIRE_LOCKED);
        fields.insert("L79", (info_bytes.len() & 0xFF) as u8);
        fields.insert("L80", ((info_bytes.len() >> 8) & 0xFF) as u8);
        fields.insert("L84", (tail_bytes.len() & 0xFF) as u8);
        fields.insert("L85", ((tail_bytes.len() >> 8) & 0xFF) as u8);

        let now_le = le_bytes(now_ms, 6);
        insert_indexed(&mut fields, 29, &now_le);
        let detect_le = le_bytes(i64::from(DETECT_FLAGS), 4);
        insert_indexed(&mut fields, 44, &detect_le);
        insert_indexed(&mut fields, 40, &ENV_TAG);
        let ink_le = le_bytes(ink, 6);
        insert_indexed(&mut fields, 60, &ink_le);
        let page_le = le_bytes(i64::from(PAGE_ID), 4);
        insert_indexed(&mut fields, 67, &page_le);
        let aid_le = le_bytes(i64::from(AID), 4);
        insert_indexed(&mut fields, 71, &aid_le);

        let chains: [(&[&'static str], [usize; 2], [usize; 3], &[u8]); 3] = [
            (&["L48", "L49", "L51"], [9, 18], [3, 11, 12], &query_digest),
            (&["L52", "L53", "L55"], [10, 19], [4, 8, 9], &body_digest),
            (&["L56", "L57", "L59"], [11, 21], [5, 12, 13], &ua_digest),
        ];
        for (slots, indices, canary_spec, digest) in chains {
            let written = chain_bytes(
                digest,
                indices,
                [canary_spec[0], canary_spec[1], canary_spec[2]],
            );
            for (slot, value) in slots.iter().zip(written) {
                fields.insert(*slot, value);
            }
        }
        fields
    }
}

fn insert_indexed(fields: &mut HashMap<&'static str, u8>, base: usize, bytes: &[u8]) {
    for (i, byte) in bytes.iter().enumerate() {
        let name = leak_field_name(base + i);
        fields.insert(name, *byte);
    }
}

/// 字段名是固定集合（L24..L85），用 match 生成 `'static str`。
fn leak_field_name(index: usize) -> &'static str {
    const NAMES: [&str; 62] = [
        "L24", "L25", "L26", "L27", "L28", "L29", "L30", "L31", "L32", "L33", "L34", "L35", "L36",
        "L37", "L38", "L39", "L40", "L41", "L42", "L43", "L44", "L45", "L46", "L47", "L48", "L49",
        "L50", "L51", "L52", "L53", "L54", "L55", "L56", "L57", "L58", "L59", "L60", "L61", "L62",
        "L63", "L64", "L65", "L66", "L67", "L68", "L69", "L70", "L71", "L72", "L73", "L74", "L75",
        "L76", "L77", "L78", "L79", "L80", "L81", "L82", "L83", "L84", "L85",
    ];
    NAMES[index - 24]
}

fn chain_bytes(digest: &[u8], indices: [usize; 2], canary_spec: [usize; 3]) -> [u8; 3] {
    [
        digest[indices[0]],
        digest[indices[1]],
        canary(digest, canary_spec[0], canary_spec[1], canary_spec[2]),
    ]
}

fn canary(digest: &[u8], offset: usize, sentinel: usize, fallback: usize) -> u8 {
    for byte in digest.iter().skip(offset) {
        if usize::from(*byte) != sentinel {
            return *byte;
        }
    }
    fallback as u8
}

fn mask_pair(
    rng: &mut dyn RandomSource,
    pair: [u8; 2],
    low: Option<u8>,
    high: Option<u8>,
) -> [u8; 4] {
    let noise = rng.next_int(65535);
    let low_value = low.unwrap_or((noise & 0xFF) as u8);
    let high_value = high.unwrap_or(((noise >> 8) & 0xFF) as u8);
    [
        (low_value & 0xAA) | (pair[0] & 0x55),
        (low_value & 0x55) | (pair[0] & 0xAA),
        (high_value & 0xAA) | (pair[1] & 0x55),
        (high_value & 0x55) | (pair[1] & 0xAA),
    ]
}

fn header_noise(rng: &mut dyn RandomSource, user_agent: &str) -> u8 {
    let base = match family_of(user_agent) {
        "chrome" => 0,
        "firefox" => 40,
        "safari" => 81,
        "edge" => 125,
        "huawei" => 170,
        _ => 210,
    };
    ((base + rng.next_int(40)) & 0xFF) as u8
}

fn probe_noise(rng: &mut dyn RandomSource) -> u8 {
    let value = rng.next_int(240);
    if value > 109 {
        (value + value % 2 + 1) as u8
    } else {
        value as u8
    }
}

fn tripwire_noise(rng: &mut dyn RandomSource) -> u8 {
    ((rng.next_int(255) as u8) & TRIPWIRE_FREE) | TRIPWIRE_SET
}

fn expand_noise(rng: &mut dyn RandomSource, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + body.len() / 3 + 4);
    let mut offset = 0;
    while offset < body.len() {
        let remaining = body.len() - offset;
        if remaining < 3 {
            out.push(body[offset]);
            if remaining > 1 && body[offset + 1] != 0 {
                out.push(body[offset + 1]);
            }
            offset += remaining;
            continue;
        }
        let noise = (rng.next_int(1000) & 0xFF) as u8;
        for i in 0..3 {
            out.push((noise & NOISE_MASKS[i]) | (body[offset + i] & DATA_MASKS[i]));
        }
        out.push(
            (body[offset] & NOISE_MASKS[0])
                | (body[offset + 1] & NOISE_MASKS[1])
                | (body[offset + 2] & NOISE_MASKS[2]),
        );
        offset += 3;
    }
    out
}

fn family_of(user_agent: &str) -> &'static str {
    let ua = user_agent.to_lowercase();
    for name in ["edg", "huawei", "firefox", "chrome", "safari"] {
        if ua.contains(name) {
            return if name == "edg" { "edge" } else { name };
        }
    }
    "other"
}

fn le_bytes(value: i64, count: usize) -> Vec<u8> {
    (0..count)
        .map(|i| ((value >> (8 * i)) & 0xFF) as u8)
        .collect()
}

fn digest_of(text: &str) -> [u8; 32] {
    let first = sm3_hash_str(&format!("{text}{SALT}"));
    crate::sm3::sm3_hash(&first)
}

/// 用户代理摘要。
pub fn user_agent_digest(user_agent: &str) -> [u8; 32] {
    let key = [0u8, 1, (DETECT_FLAGS & 0xFF) as u8];
    let raw = js_bytes(user_agent.trim());
    let sealed = rc4(&key, &raw);
    sm3_hash_str(&encode_base64(&sealed, ALPHABET_S3))
}

/// SDK 版 RC4（S-box 逆序初始化 + 乘法密钥调度），逐字节移植。
pub fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut box_: [u8; 256] = [0; 256];
    for i in 0..256 {
        box_[255 - i] = i as u8;
    }
    let mut j: usize = 0;
    for i in 0..256 {
        j = (j * usize::from(box_[i]) + j + usize::from(key[i % key.len()])) % 256;
        box_.swap(i, j);
    }
    let mut out = vec![0u8; data.len()];
    let mut i: usize = 0;
    j = 0;
    for index in 0..data.len() {
        i = (i + 1) % 256;
        j = (j + usize::from(box_[i])) % 256;
        box_.swap(i, j);
        out[index] = data[index] ^ box_[(usize::from(box_[i]) + usize::from(box_[j])) % 256];
    }
    out
}

/// 自定义字母表 base64（含 `=` 补齐）。
pub fn encode_base64(data: &[u8], alphabet: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut offset = 0;
    while offset < data.len() {
        let remaining = data.len() - offset;
        let mut block: u32 = 0;
        for i in 0..3 {
            block <<= 8;
            if i < remaining {
                block |= u32::from(data[offset + i]);
            }
        }
        let digits = [
            ((block >> 18) & 0x3F) as usize,
            ((block >> 12) & 0x3F) as usize,
            ((block >> 6) & 0x3F) as usize,
            (block & 0x3F) as usize,
        ];
        out.push(alphabet[digits[0]] as char);
        out.push(alphabet[digits[1]] as char);
        if remaining >= 2 {
            out.push(alphabet[digits[2]] as char);
        }
        if remaining >= 3 {
            out.push(alphabet[digits[3]] as char);
        }
        offset += 3;
    }
    let pad = (4 - out.len() % 4) % 4;
    out.extend(std::iter::repeat_n('=', pad));
    out
}

/// JS 字符串转字节：UTF-16 码元大于 0xFF 时输出高、低两字节。
pub fn js_bytes(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for unit in text.encode_utf16() {
        if unit & 0xFF00 != 0 {
            out.push((unit >> 8) as u8);
        }
        out.push((unit & 0xFF) as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY: &str = "device_platform=webapp&aid=6383&channel=channel_pc_web&aweme_id=7704322116207906090&pc_client_type=1&version_code=190500&version_name=19.5.0&cookie_enabled=true&screen_width=1920&screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=146.0.0.0&browser_online=true&engine_name=Blink&engine_version=146.0.0.0&os_name=Windows&os_version=10&cpu_core_num=8&device_memory=8&platform=PC&msToken=AbCdEf0123456789-_==";
    const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";

    struct ScriptedRandom {
        values: Vec<u32>,
        cursor: usize,
    }

    impl ScriptedRandom {
        fn new(count: usize) -> Self {
            let values = (0..count)
                .map(|i| ((i as u64 * 104729 + 7919) % 1_000_003 + 1) as u32)
                .collect();
            Self { values, cursor: 0 }
        }
    }

    impl RandomSource for ScriptedRandom {
        fn next_int(&mut self, max: u32) -> u32 {
            let value = self.values[self.cursor % self.values.len()];
            self.cursor += 1;
            value % max
        }
    }

    #[test]
    fn matches_reference_output() {
        let mut rng = ScriptedRandom::new(600);
        let mut bogus = ABogus::new(UA, &mut rng);
        let value = bogus.get_value(QUERY, "", 1_763_000_000_000).unwrap();
        assert_eq!(
            value,
            "mXUjkz7jQq8jCdFGmOGmHtClJ9nMrPuyz4Txbt3P9OYscZMPAmNOPPcWcxLws0ffERBzi91HhxMMYEVcstXkZHHpqmpfuQwWds55Vg6L/qwmGzJQLNj0eLfzKwMx0RGqaQ5GiAWI/UrogjVAkrdY/BAy7KoC55uBM3xSk/zbT9Bg1FyAD1n3PQtkLh4r"
        );
        assert_eq!(rng.cursor, 38);
    }

    #[test]
    fn user_agent_digest_matches_reference() {
        let digest = user_agent_digest("Mozilla/5.0 Test UA 测试");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "5730f0bb0542f053b281ecc19c7e7ac8dc81d1f9aead18a2a5acd6fd33d983ac"
        );
    }

    #[test]
    fn base64_matches_reference() {
        let s3: Vec<u8> = (0..37u32).map(|i| ((i * 7) & 0xFF) as u8).collect();
        assert_eq!(
            encode_base64(&s3, ALPHABET_S3),
            "ccW/hvjNUN1VMy20Xhz75gkHlYI8LGOiOUC9FW3ByzqEGCaxec=="
        );
        let s4: Vec<u8> = (0..37u32).map(|i| ((i * 11) & 0xFF) as u8).collect();
        assert_eq!(
            encode_base64(&s4, ALPHABET_S4),
            "DDuImVjHfL7RR9GGimC5T-daJzZWGeBtdk8AQ3feVwXE5H5kND=="
        );
    }

    #[test]
    fn rc4_matches_reference() {
        let out = rc4(&[0, 1, 14], &[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(out, vec![177, 168, 168, 123, 18, 136, 252, 117, 109]);
    }
}
