//! 跨平台安全文件名。

use std::path::Path;

/// 清洗文件名：替换非法/控制字符、压缩空白、去掉结尾点与空格、
/// 按字符数与 UTF-8 字节数截断（ext4 单文件上限 255 字节）。
pub fn safe_file_name(input: &str, max_chars: usize, max_bytes: usize) -> String {
    let mut name = input.trim().to_string();
    name = name
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                || (c as u32) < 0x20
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    name = collapse_whitespace(&name);
    name = name.trim_end_matches(['.', ' ']).to_string();

    let mut out = String::new();
    let mut count = 0;
    let mut bytes = 0;
    for ch in name.chars() {
        if count >= max_chars {
            break;
        }
        let len = ch.len_utf8();
        if bytes + len > max_bytes {
            break;
        }
        out.push(ch);
        count += 1;
        bytes += len;
    }
    let out = out.trim_end().to_string();

    if out.is_empty() {
        return "download".to_string();
    }
    let lower = out.to_ascii_lowercase();
    let reserved = matches!(
        lower.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    );
    if reserved {
        format!("_{out}")
    } else {
        out
    }
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

/// 目录下重名时追加 `_1`、`_2`……
pub fn unique_file_name(dir: &Path, base: &str, extension: &str) -> String {
    let mut candidate = base.to_string();
    let mut counter = 1;
    while dir.join(format!("{candidate}.{extension}")).exists() {
        candidate = format!("{base}_{counter}");
        counter += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_chars_and_bytes() {
        let long = "超长?标题/测试".repeat(40);
        let name = safe_file_name(&long, 100, 200);
        assert!(name.chars().count() <= 100);
        assert!(name.len() <= 200);
        assert!(!name.chars().any(|c| r#"<>:"/\|?*"#.contains(c)));
    }

    #[test]
    fn handles_reserved_and_empty() {
        assert_eq!(safe_file_name("   ", 100, 200), "download");
        assert_eq!(safe_file_name("CON", 100, 200), "_CON");
        assert_eq!(safe_file_name("结尾点...", 100, 200), "结尾点");
        assert_eq!(safe_file_name("a   b c", 100, 200), "a b c");
    }
}
