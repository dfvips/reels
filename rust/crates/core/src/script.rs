//! 下载脚本导出：每类工具一个脚本，包含全部链接。

use crate::files::safe_file_name;

pub const TOOLS: [&str; 3] = ["aria2c", "downloader", "RobustDownloader"];

const DOUYIN_REFERER: &str = "https://www.douyin.com/";
const XHS_REFERER: &str = "https://www.xiaohongshu.com/";
const UA_WINDOWS: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const UA_MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Debug, Clone)]
pub struct GeneratedScript {
    pub file_name: String,
    pub contents: String,
}

/// 一条待下载的链接；同标题的多条链接通过下载文件名序号区分。
#[derive(Debug, Clone)]
pub struct ScriptEntry {
    pub title: String,
    pub url: String,
    pub extension: Option<String>,
}

/// 为每类工具生成一个脚本文件，脚本内包含全部链接的下载命令。
///
/// 脚本文件名：首标题前 10 字（多链接时拼接「 - 末标题前 10 字」）+ `_工具名`。
/// 脚本内的下载文件名：安全化标题（重名追加 `_2` 起的序号）+ 扩展名。
pub fn build_scripts(entries: &[ScriptEntry], is_windows: bool) -> Vec<GeneratedScript> {
    let script_ext = if is_windows { ".bat" } else { ".sh" };
    let titles: Vec<&str> = entries
        .iter()
        .map(|entry| {
            if entry.title.trim().is_empty() {
                "抖音视频"
            } else {
                entry.title.as_str()
            }
        })
        .collect();
    let base_name = build_script_base_name(&titles);
    let mut used_names: Vec<String> = Vec::new();
    let mut commands: Vec<Vec<String>> = TOOLS.iter().map(|_| Vec::new()).collect();
    for entry in entries {
        let title = if entry.title.trim().is_empty() {
            "抖音视频"
        } else {
            entry.title.as_str()
        };
        let download_ext = entry
            .extension
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("mp4");
        let name = safe_file_name(title, 80, 200);
        let mut download_name = name.clone();
        let mut suffix = 2;
        while used_names.contains(&download_name) {
            download_name = format!("{name}_{suffix}");
            suffix += 1;
        }
        used_names.push(download_name.clone());
        let download_name = format!("{download_name}.{download_ext}");
        for (tool_index, tool) in TOOLS.iter().enumerate() {
            commands[tool_index].push(build_command(
                tool,
                &entry.url,
                &download_name,
                is_windows,
            ));
        }
    }
    TOOLS
        .iter()
        .enumerate()
        .map(|(tool_index, tool)| {
            let contents = build_script(&commands[tool_index], is_windows);
            GeneratedScript {
                file_name: format!("{base_name}_{tool}{script_ext}"),
                contents,
            }
        })
        .collect()
}

fn build_script_base_name(titles: &[&str]) -> String {
    let Some(first) = titles.first() else {
        return shorten("抖音视频");
    };
    let base = shorten(first);
    match titles.len() {
        1 => base,
        _ => {
            let last = shorten(titles[titles.len() - 1]);
            format!("{base} - {last}")
        }
    }
}

fn shorten(title: &str) -> String {
    let safe = safe_file_name(title, 100, 200);
    let mut chars = safe.chars();
    let head: String = chars.by_ref().take(10).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

fn build_script(commands: &[String], is_windows: bool) -> String {
    let line_end = if is_windows { "\r\n" } else { "\n" };
    let mut out = String::new();
    out.push_str(if is_windows {
        "chcp 65001 >nul"
    } else {
        "#!/bin/bash"
    });
    out.push_str(line_end);
    out.push_str(line_end);
    for command in commands {
        out.push_str(command);
        out.push_str(line_end);
    }
    out
}

fn build_command(tool: &str, url: &str, file_name: &str, is_windows: bool) -> String {
    let user_agent = if is_windows { UA_WINDOWS } else { UA_MAC };
    let referer = if url.contains("xhscdn.com") || url.contains("xiaohongshu.com") {
        XHS_REFERER
    } else {
        DOUYIN_REFERER
    };
    let header_args = format!(
        "--header {} --header {}",
        quote(&format!("User-Agent:{user_agent}"), is_windows),
        quote(&format!("Referer:{referer}"), is_windows)
    );
    let quoted_url = quote(url, is_windows);
    let quoted_name = quote(file_name, is_windows);
    match tool {
        "aria2c" => {
            format!("aria2c -c -s 16 -x 16 -k 1M -o {quoted_name} {quoted_url} {header_args}")
        }
        "downloader" => format!("downloader {quoted_url} {quoted_name} 8 16 {header_args}"),
        "RobustDownloader" => {
            let exe = if is_windows {
                "RobustDownloader.exe"
            } else {
                "RobustDownloader"
            };
            format!(
                "{exe} --add {quoted_url} --dir \".\" --name {quoted_name} --threads 16 --block-size 1 {header_args} --start --silent"
            )
        }
        _ => String::new(),
    }
}

fn quote(value: &str, is_windows: bool) -> String {
    if is_windows {
        format!("\"{}\"", value.replace('%', "%%"))
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, url: &str) -> ScriptEntry {
        ScriptEntry {
            title: title.to_string(),
            url: url.to_string(),
            extension: None,
        }
    }

    #[test]
    fn single_entry_has_no_sequence() {
        let scripts = build_scripts(
            &[entry(
                "城市开启，吴磊在WGALA奔赴自己的黄金时代",
                "https://x?a=1",
            )],
            false,
        );
        assert_eq!(scripts.len(), TOOLS.len());
        assert!(scripts[0]
            .file_name
            .starts_with("城市开启，吴磊在WG..._aria2c.sh"));
        assert!(scripts[0]
            .contents
            .contains("'城市开启，吴磊在WGALA奔赴自己的黄金时代.mp4'"));
    }

    #[test]
    fn same_title_entries_get_sequence_numbers() {
        let scripts = build_scripts(
            &[
                entry("把手机借给我，不是把手机给我", "https://x/1"),
                entry("把手机借给我，不是把手机给我", "https://x/2"),
            ],
            false,
        );
        assert_eq!(scripts.len(), TOOLS.len());
        let names: Vec<&str> = scripts.iter().map(|s| s.file_name.as_str()).collect();
        assert!(names.contains(&"把手机借给我，不是把... - 把手机借给我，不是把..._aria2c.sh"));
        let aria = &scripts[0];
        assert!(aria.contents.contains("'把手机借给我，不是把手机给我.mp4'"));
        assert!(aria.contents.contains("'把手机借给我，不是把手机给我_2.mp4'"));
    }

    #[test]
    fn multi_titles_join_first_and_last_in_file_name() {
        let scripts = build_scripts(
            &[
                entry("第一个标题很长很长很长", "https://x/1"),
                entry("中间标题", "https://x/2"),
                entry("最后的标题也很长很长", "https://x/3"),
            ],
            false,
        );
        assert_eq!(scripts.len(), TOOLS.len());
        assert_eq!(
            scripts[0].file_name,
            "第一个标题很长很长很... - 最后的标题也很长很长_aria2c.sh"
        );
        let names: Vec<&str> = scripts.iter().map(|s| s.file_name.as_str()).collect();
        assert!(names.iter().all(|name| name.contains(" - ")));
    }

    #[test]
    fn bat_uses_crlf_and_escapes_percent() {
        let scripts = build_scripts(&[entry("100% 测试", "https://x?a=1&b=2")], true);
        assert_eq!(scripts.len(), TOOLS.len());
        let bat = &scripts[0];
        assert!(bat.contents.starts_with("chcp 65001 >nul\r\n\r\n"));
        assert!(bat.contents.contains("100%%"));
        assert!(bat.contents.ends_with("\r\n"));
        assert!(bat.file_name.ends_with("_aria2c.bat"));
    }

    #[test]
    fn every_tool_script_contains_all_entries() {
        let scripts = build_scripts(
            &[
                entry("a", "https://x/1"),
                entry("b", "https://x/2"),
            ],
            false,
        );
        for script in &scripts {
            assert!(script.contents.contains("'https://x/1'"));
            assert!(script.contents.contains("'https://x/2'"));
        }
    }

    #[test]
    fn sh_uses_single_quotes() {
        let scripts = build_scripts(&[entry("t", "https://x?a=1")], false);
        let sh = &scripts[1];
        assert!(sh.contents.starts_with("#!/bin/bash\n\n"));
        assert!(sh.contents.contains("downloader 'https://x?a=1'"));
    }

    #[test]
    fn extensions_and_referer_are_respected() {
        let scripts = build_scripts(
            &[ScriptEntry {
                title: "小红书图片".to_string(),
                url: "https://sns-img-hw.xhscdn.com/abc".to_string(),
                extension: Some("jpg".to_string()),
            }],
            false,
        );
        let aria = &scripts[0];
        assert!(aria.contents.contains("'小红书图片.jpg'"));
        assert!(aria
            .contents
            .contains("Referer:https://www.xiaohongshu.com/"));
    }

    #[test]
    fn image_extension_is_kept_in_script_names() {
        let scripts = build_scripts(
            &[ScriptEntry {
                title: "livephoto 图".to_string(),
                url: "https://sns-img-hw.xhscdn.com/abc".to_string(),
                extension: Some("heic".to_string()),
            }],
            false,
        );
        assert!(scripts[0].contents.contains("'livephoto 图.heic'"));
        assert!(scripts[0].contents.contains("'https://sns-img-hw.xhscdn.com/abc'"));
    }
}
