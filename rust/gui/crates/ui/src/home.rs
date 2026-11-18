//! 首页：链接输入、解析结果、复制 / 导出脚本 / 下载。

use std::{path::PathBuf, time::Duration};

/// 阻塞式原生目录选择框。
///
/// 必须在实体 update 之外调用（例如放进 `cx.spawn` 任务里）：
/// 模态循环期间 GPUI 若是持有 App 借用，定时任务（toast 等）的
/// update 会触发 "RefCell already borrowed" panic。
pub fn pick_directory_blocking(initial: Option<&std::path::Path>) -> Option<PathBuf> {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        let mut dialog = rfd::FileDialog::new();
        if let Some(directory) = initial.filter(|directory| directory.exists()) {
            dialog = dialog.set_directory(directory);
        }
        dialog.pick_folder()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = initial;
        None
    }
}

fn format_duration(total_seconds: u64) -> String {
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    if minutes >= 60 {
        let hours = minutes / 60;
        let minutes = minutes % 60;
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// 体验版“导出与下载”只处理当前卡片，导出方式弹窗打开期间暂存其条目。
#[cfg(target_os = "windows")]
pub struct SingleExport {
    pub title: String,
    pub entries: Vec<MediaEntry>,
}

use douyin_core::extractor::{MediaEntry, MediaKind, MediaPlatform, VideoExtractor};
use douyin_input::{Input, InputSize, MULTILINE_LINE_HEIGHT};
use gpui::{
    div, px, AnyElement, ClipboardItem, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, TextRun, WeakEntity, Window,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use gpui::{Modifiers, MouseButton, MouseUpEvent, PlatformInput};

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use crate::download_flow::has_multiple_video_qualities;
use crate::paths::{format_size, is_desktop};
use crate::theme::Palette;
use crate::widgets::{self, ActionButton};
use crate::{icons, AppView, Dialog, ResourceScope, ResultItem, ToastKind, MOBILE_APPBAR_HEIGHT};

const DEFAULT_HOME_INPUT_LINES: usize = 6;
const MIN_COMPACT_HOME_INPUT_LINES: usize = 1;
const HOME_ACTION_ROW_HEIGHT: f32 = 44.0;
const COMPACT_INPUT_CHROME_HEIGHT: f32 = 28.0;
const COMPACT_INPUT_SAFETY_HEIGHT: f32 = 8.0;
#[cfg(target_os = "ios")]
const IOS_INPUT_CHROME_HEIGHT: f32 = crate::ios_native_input::TEXT_PADDING * 2.0;
fn normal_max_lines_for_height(visible_height: f32, keyboard_height: f32) -> usize {
    let height_above_keyboard = (visible_height - keyboard_height).max(1.0);
    ((height_above_keyboard * 0.5 - COMPACT_INPUT_CHROME_HEIGHT) / MULTILINE_LINE_HEIGHT)
        .floor()
        .max(MIN_COMPACT_HOME_INPUT_LINES as f32) as usize
}
#[cfg(target_os = "ios")]
fn ios_max_lines_for_window_height(window_height: f32) -> usize {
    let max_height = (window_height / 3.0).max(IOS_INPUT_CHROME_HEIGHT + MULTILINE_LINE_HEIGHT);
    (((max_height - IOS_INPUT_CHROME_HEIGHT) / MULTILINE_LINE_HEIGHT).floor() as usize)
        .max(MIN_COMPACT_HOME_INPUT_LINES)
}
#[cfg(target_os = "android")]
fn android_max_lines_for_window_height(window_height: f32) -> usize {
    let max_height = (window_height / 3.0).max(COMPACT_INPUT_CHROME_HEIGHT + MULTILINE_LINE_HEIGHT);
    (((max_height - COMPACT_INPUT_CHROME_HEIGHT) / MULTILINE_LINE_HEIGHT).floor() as usize)
        .max(MIN_COMPACT_HOME_INPUT_LINES)
}
/// The navigation gesture area is included in the bottom inset even when the
/// IME is hidden. A keyboard occupies substantially more than this.
#[cfg(target_os = "android")]
const HOME_IME_BOTTOM_INSET_THRESHOLD: f32 = 96.0;

struct TitleWrapLine {
    text: String,
    width: f32,
}

fn text_width(window: &Window, text: &str, size: f32, family: &str, color: gpui::Hsla) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let runs = [TextRun {
        len: text.len(),
        font: gpui::font(family),
        color,
        ..Default::default()
    }];
    window
        .text_system()
        .layout_line(text, px(size), &runs, None)
        .width
        .into()
}

fn input_visual_line_count(
    window: &Window,
    text: &str,
    wrap_width: f32,
    palette: Palette,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    let runs = [TextRun {
        len: text.len(),
        font: gpui::font(crate::theme::ui_font()),
        color: palette.text.into(),
        ..Default::default()
    }];
    window
        .text_system()
        .shape_text(
            text.to_owned().into(),
            px(16.0),
            &runs,
            Some(px(wrap_width.max(40.0))),
            None,
        )
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum::<usize>()
                .max(1)
        })
        .unwrap_or_else(|_| text.lines().count().max(1))
}

fn title_meta_width(window: &Window, kind_label: &str, summary: &str, palette: Palette) -> f32 {
    let color: gpui::Hsla = palette.text.into();
    let badge = text_width(window, kind_label, 11.0, crate::theme::ui_font(), color) + 14.0;
    let summary = if summary.is_empty() {
        0.0
    } else {
        8.0 + text_width(window, summary, 11.0, crate::theme::ui_font(), color)
    };
    8.0 + badge + summary + 4.0
}

fn wrap_title_lines(
    window: &mut Window,
    title: &str,
    wrap_width: f32,
    palette: Palette,
) -> Vec<TitleWrapLine> {
    let text = title.replace(['\r', '\n'], " ");
    if text.is_empty() {
        return vec![TitleWrapLine { text, width: 0.0 }];
    }

    let shared: SharedString = text.clone().into();
    let runs = [TextRun {
        len: shared.len(),
        font: gpui::font(crate::theme::mono_font()),
        color: palette.text.into(),
        ..Default::default()
    }];
    let Ok(lines) = window.text_system().shape_text(
        shared.clone(),
        px(12.0),
        &runs,
        Some(px(wrap_width.max(80.0))),
        None,
    ) else {
        return vec![TitleWrapLine { text, width: 0.0 }];
    };
    let Some(line) = lines.first() else {
        return vec![TitleWrapLine { text, width: 0.0 }];
    };

    let total_width: f32 = line.unwrapped_layout.width.into();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut start_x = 0.0_f32;
    for boundary in line.wrap_boundaries() {
        let Some(run) = line.runs().get(boundary.run_ix) else {
            continue;
        };
        let Some(glyph) = run.glyphs.get(boundary.glyph_ix) else {
            continue;
        };
        let end = glyph.index.min(shared.len());
        if end <= start {
            continue;
        }
        let x: f32 = glyph.position.x.into();
        let raw = &shared.as_ref()[start..end];
        segments.push(TitleWrapLine {
            text: if segments.is_empty() {
                raw.to_string()
            } else {
                raw.trim_start_matches(' ').to_string()
            },
            width: x - start_x,
        });
        start = end;
        start_x = x;
    }

    let raw = &shared.as_ref()[start..];
    segments.push(TitleWrapLine {
        text: if segments.is_empty() {
            raw.to_string()
        } else {
            raw.trim_start_matches(' ').to_string()
        },
        width: total_width - start_x,
    });

    segments
}

impl AppView {
    /// 链接卡片第二行：清晰度 / 大小 / 分辨率 / 时长。
    fn entry_meta(&self, entry: &MediaEntry) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(quality) = entry.quality.as_ref().filter(|quality| !quality.is_empty()) {
            parts.push(quality.clone());
        }
        if let Some(size) = entry.size_bytes {
            parts.push(format_size(size));
        }
        if let (Some(width), Some(height)) = (entry.width, entry.height) {
            parts.push(format!("{width}x{height}"));
        }
        if let Some(duration) = entry.duration_secs {
            parts.push(format!("时长 {}", format_duration(duration)));
        }
        parts.join(" · ")
    }

    pub fn parse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.clone();
        if text.trim().is_empty() {
            self.toast("请输入短视频链接", ToastKind::Error, cx);
            return;
        }
        crate::hide_soft_keyboard();
        self.loading = true;
        self.results.clear();
        crate::dialogs::close_or_replace_dialog(self, window, cx);
        self.spinner_phase = 0.0;
        cx.notify();

        cx.spawn(async move |this: WeakEntity<AppView>, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            let keep_going = this
                .update(cx, |this, cx| {
                    if this.loading {
                        this.spinner_phase = (this.spinner_phase + 0.125) % 1.0;
                        cx.notify();
                    }
                    this.loading
                })
                .unwrap_or(false);
            if !keep_going {
                break;
            }
        })
        .detach();

        let xhs_cookie = if self.settings.xhs_cookie.trim().is_empty() {
            None
        } else {
            Some(self.settings.xhs_cookie.clone())
        };
        cx.spawn(async move |this: WeakEntity<AppView>, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    match VideoExtractor::new().map(|e| e.with_xhs_cookie(xhs_cookie)) {
                        Ok(extractor) => Ok(extractor.extract_multiple(&text)),
                        Err(error) => Err(format!("初始化网络客户端失败: {error}")),
                    }
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match results {
                    Ok(results) => this.apply_results(results, cx),
                    Err(error) => {
                        this.toast(format!("解析失败: {error}"), ToastKind::Error, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_results(
        &mut self,
        results: Vec<douyin_core::extractor::ExtractionResult>,
        cx: &mut Context<Self>,
    ) {
        let mut items = Vec::new();
        let mut errors = Vec::new();
        for result in results {
            if result.success {
                if !result.entries.is_empty() {
                    let title = result
                        .title
                        .filter(|title| !title.trim().is_empty())
                        .unwrap_or_else(|| match result.platform {
                            MediaPlatform::Douyin => match result.media_kind {
                                MediaKind::Image => format!("抖音图文_{}", items.len() + 1),
                                MediaKind::Video => format!("抖音视频_{}", items.len() + 1),
                            },
                            MediaPlatform::Xhs => format!("小红书笔记_{}", items.len() + 1),
                        });
                    items.push(ResultItem {
                        title,
                        platform: result.platform,
                        media_kind: result.media_kind,
                        entries: result.entries,
                        covers: result.covers,
                    });
                }
            } else if let Some(error) = result.error {
                errors.push(error);
            }
        }

        if items.is_empty() {
            let message = if errors.is_empty() {
                "未找到有效的视频链接".to_string()
            } else {
                format!("解析失败: {}", errors.join(", "))
            };
            self.toast(message, ToastKind::Error, cx);
            return;
        }

        let count: usize = items.iter().map(ResultItem::entry_count).sum();
        self.results = items;
        self.experience_item = 0;
        self.experience_tab = crate::ExperienceTab::All;
        self.experience_slide = 0;
        self.reset_experience_slide_gesture();
        self.experience_quality.clear();
        self.experience_quality_menu = false;
        self.toast(format!("成功解析 {count} 个链接"), ToastKind::Info, cx);
        self.navigate_to(crate::Route::Results, cx);
    }

    pub fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "ios")]
        {
            let _ = cx;
            crate::ios_native_input::paste("url-input");
            return;
        }
        #[cfg(not(target_os = "ios"))]
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                self.input = text;
                cx.notify();
            }
        }
    }

    pub fn clear_input(&mut self, cx: &mut Context<Self>) {
        self.input.clear();
        #[cfg(target_os = "ios")]
        {
            self.home_native_input_lines = None;
            crate::ios_native_input::replace_native_text("url-input", "");
            crate::ios_native_input::clear_height_events("url-input");
        }
        #[cfg(target_os = "android")]
        gpui_android::clear_native_editor_text();
        self.input_scroll
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        cx.notify();
    }

    pub fn copy_all(&mut self, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            self.toast("没有可复制的内容", ToastKind::Error, cx);
            return;
        }
        let text = self
            .results
            .iter()
            .flat_map(|item| {
                let mut lines = vec![item.title.clone()];
                lines.extend(item.entries.iter().map(|entry| entry.url.clone()));
                lines
            })
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast("已复制所有链接到剪贴板", ToastKind::Info, cx);
    }

    pub fn copy_entry(&mut self, index: usize, entry_index: usize, cx: &mut Context<Self>) {
        let Some(entry) = self
            .results
            .get(index)
            .and_then(|item| item.entries.get(entry_index))
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(entry.url.clone()));
        self.toast("已复制链接", ToastKind::Success, cx);
    }

    #[cfg(target_os = "macos")]
    fn set_export_tooltip_hover(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.export_tooltip_seq = self.export_tooltip_seq.wrapping_add(1);
        let seq = self.export_tooltip_seq;
        if hovered {
            if !self.export_tooltip_visible {
                self.export_tooltip_visible = true;
                cx.notify();
            }
            return;
        }
        cx.spawn(async move |this: WeakEntity<AppView>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(160))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.export_tooltip_seq == seq && this.export_tooltip_visible {
                    this.export_tooltip_visible = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn export_scripts(&mut self, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            self.toast("没有可导出的内容", ToastKind::Error, cx);
            return;
        }
        #[cfg(target_os = "windows")]
        {
            self.pending_single_export = None;
            self.dialog = Some(Dialog::ExportMethod);
            cx.notify();
            return;
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let has_quality_choice = self.results.iter().any(has_multiple_video_qualities);
            if has_quality_choice {
                self.dialog = Some(Dialog::ExportSelection);
            } else {
                self.confirm_export_scripts(ResourceScope::All, cx);
                return;
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        {
            self.dialog = Some(Dialog::ExportSelection);
        }
        #[cfg(not(target_os = "windows"))]
        cx.notify();
    }

    pub fn confirm_export_scripts(&mut self, scope: ResourceScope, cx: &mut Context<Self>) {
        let entries = self.result_script_entries(scope);
        self.confirm_export_entries(entries, cx);
    }

    fn result_script_entries(&self, scope: ResourceScope) -> Vec<douyin_core::script::ScriptEntry> {
        self.results
            .iter()
            .flat_map(|item| {
                let selected = item.select_entries(scope);
                selected
                    .into_iter()
                    .map(move |entry| douyin_core::script::ScriptEntry {
                        title: item.title.clone(),
                        url: entry.url.clone(),
                        extension: entry.extension.clone(),
                    })
            })
            .collect()
    }

    /// 导出指定条目：沿用上次目录，否则弹出目录选择框。
    pub fn confirm_export_entries(
        &mut self,
        entries: Vec<douyin_core::script::ScriptEntry>,
        cx: &mut Context<Self>,
    ) {
        self.dialog = None;
        if entries.is_empty() {
            self.toast("没有可导出的内容", ToastKind::Error, cx);
            return;
        }
        if !self.settings.last_directory.is_empty() {
            let directory = PathBuf::from(&self.settings.last_directory);
            self.export_entries_to(directory, entries, cx);
            return;
        }
        let initial = self
            .paths
            .default_download_dir
            .exists()
            .then(|| self.paths.default_download_dir.clone());
        cx.spawn(async move |this: WeakEntity<AppView>, cx| {
            let picked = pick_directory_blocking(initial.as_deref());
            let _ = this.update(cx, |this, cx| match picked {
                Some(directory) => {
                    this.settings.last_directory = directory.display().to_string();
                    this.save_settings();
                    this.export_entries_to(directory, entries, cx);
                }
                None => this.toast("导出已取消", ToastKind::Info, cx),
            });
        })
        .detach();
    }

    #[cfg(target_os = "windows")]
    pub fn choose_export_scripts(&mut self, cx: &mut Context<Self>) {
        if let Some(single) = self.pending_single_export.take() {
            let entries = single
                .entries
                .iter()
                .map(|entry| douyin_core::script::ScriptEntry {
                    title: single.title.clone(),
                    url: entry.url.clone(),
                    extension: entry.extension.clone(),
                })
                .collect();
            self.confirm_export_entries(entries, cx);
            return;
        }
        let has_quality_choice = self.results.iter().any(has_multiple_video_qualities);
        if has_quality_choice {
            self.dialog = Some(Dialog::ExportSelection);
            cx.notify();
        } else {
            self.confirm_export_scripts(ResourceScope::All, cx);
        }
    }

    #[cfg(target_os = "windows")]
    pub fn request_idm_export(&mut self, cx: &mut Context<Self>) {
        if let Some(single) = self.pending_single_export.take() {
            let links =
                Self::idm_links_for(&single.title, &single.entries, &self.settings.xhs_cookie);
            self.finish_idm_export(links, cx);
            return;
        }
        let has_quality_choice = self.results.iter().any(has_multiple_video_qualities);
        if has_quality_choice {
            self.dialog = Some(Dialog::IdmSelection);
            cx.notify();
        } else {
            self.confirm_idm_export(ResourceScope::All, cx);
        }
    }

    #[cfg(target_os = "windows")]
    pub fn confirm_idm_export(&mut self, scope: ResourceScope, cx: &mut Context<Self>) {
        self.dialog = None;
        let xhs_cookie = self.settings.xhs_cookie.clone();
        let mut links = Vec::new();
        for item in &self.results {
            let entries: Vec<MediaEntry> =
                item.select_entries(scope).into_iter().cloned().collect();
            links.extend(Self::idm_links_for(&item.title, &entries, &xhs_cookie));
        }
        self.finish_idm_export(links, cx);
    }

    #[cfg(target_os = "windows")]
    fn idm_links_for(title: &str, entries: &[MediaEntry], xhs_cookie: &str) -> Vec<crate::IdmLink> {
        let mut used_names = std::collections::HashSet::new();
        entries
            .iter()
            .map(|entry| {
                let extension = entry
                    .extension
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("mp4");
                let base = douyin_core::files::safe_file_name(title, 80, 200);
                let mut name = format!("{base}.{extension}");
                let mut suffix = 2;
                while used_names.contains(&name) {
                    name = format!("{base}_{suffix}.{extension}");
                    suffix += 1;
                }
                used_names.insert(name.clone());
                let referer =
                    if entry.url.contains("xhscdn.com") || entry.url.contains("xiaohongshu.com") {
                        "https://www.xiaohongshu.com/"
                    } else {
                        "https://www.douyin.com/"
                    };
                let cookie = (!xhs_cookie.is_empty() && referer == "https://www.xiaohongshu.com/")
                    .then(|| xhs_cookie.to_string());
                crate::IdmLink {
                    url: entry.url.clone(),
                    referer: referer.to_string(),
                    cookie,
                    file_name: name,
                }
            })
            .collect()
    }

    #[cfg(target_os = "windows")]
    fn finish_idm_export(&mut self, links: Vec<crate::IdmLink>, cx: &mut Context<Self>) {
        if links.is_empty() {
            self.show_idm_result(false, "没有可导出的内容".to_string(), cx);
            return;
        }
        let result = crate::idm::export_links(&links);
        self.show_idm_result(
            result.is_ok(),
            result
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default(),
            cx,
        );
    }

    #[cfg(target_os = "windows")]
    fn show_idm_result(&mut self, success: bool, message: String, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::IdmResult { success, message });
        cx.notify();
    }

    fn export_entries_to(
        &mut self,
        directory: PathBuf,
        entries: Vec<douyin_core::script::ScriptEntry>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = std::fs::create_dir_all(&directory) {
            self.toast(format!("导出下载脚本失败: {error}"), ToastKind::Error, cx);
            return;
        }
        let scripts = douyin_core::script::build_scripts(&entries, cfg!(target_os = "windows"));
        let mut written = 0;
        let mut first_error = None;
        for script in &scripts {
            match std::fs::write(directory.join(&script.file_name), &script.contents) {
                Ok(()) => written += 1,
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        if written == 0 {
            self.toast(
                format!(
                    "导出下载脚本失败: {}",
                    first_error
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| directory.display().to_string())
                ),
                ToastKind::Error,
                cx,
            );
        } else {
            self.toast(
                format!("已导出 {written} 个下载脚本到 {}", directory.display()),
                ToastKind::Success,
                cx,
            );
        }
    }

    pub fn render_home(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = self.render_home_content(palette, window, cx);
        div()
            .id("home-scroll")
            .flex_1()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&self.home_scroll)
            .child(content)
            .into_any_element()
    }

    /// When the IME leaves less room than the normal home layout needs,
    /// give the editor the whole visible viewport. The returned value is the
    /// number of complete text rows that fit in that space.
    pub fn compact_home_input_lines(&self, window: &Window) -> Option<usize> {
        if is_desktop() || !self.input_focus.is_focused(window) {
            return None;
        }
        #[cfg(target_os = "android")]
        if gpui_android::ime_bottom_inset_logical() <= HOME_IME_BOTTOM_INSET_THRESHOLD {
            return None;
        }
        let full_visible_height: f32 = window.fully_visible_bounds().size.height.into();
        #[cfg(target_os = "ios")]
        let visible_height = if Self::home_ime_visible(window) {
            (full_visible_height - gpui_ios::keyboard_height()).max(1.0)
        } else {
            full_visible_height
        };
        #[cfg(not(target_os = "ios"))]
        let visible_height = full_visible_height;
        let default_input_height = DEFAULT_HOME_INPUT_LINES as f32 * MULTILINE_LINE_HEIGHT + 24.0;
        let normal_layout_height = MOBILE_APPBAR_HEIGHT
            + 16.0
            + default_input_height
            + 16.0
            + HOME_ACTION_ROW_HEIGHT
            + 16.0;
        if visible_height >= normal_layout_height {
            return None;
        }

        // In compact mode the app bar and the home column's vertical padding
        // remain visible. Count rows only from the space left after those fixed
        // elements, the input chrome, and a small keyboard-edge safety gap.
        let usable_height = (visible_height
            - MOBILE_APPBAR_HEIGHT
            - 32.0
            - COMPACT_INPUT_SAFETY_HEIGHT
            - COMPACT_INPUT_CHROME_HEIGHT)
            .max(0.0);
        let lines = (usable_height / MULTILINE_LINE_HEIGHT).floor() as usize;
        Some(lines.max(MIN_COMPACT_HOME_INPUT_LINES))
    }

    fn home_ime_visible(window: &Window) -> bool {
        if is_desktop() || !cfg!(any(target_os = "android", target_os = "ios")) {
            return false;
        }
        #[cfg(target_os = "android")]
        {
            let _ = window;
            gpui_android::ime_bottom_inset_logical() > HOME_IME_BOTTOM_INSET_THRESHOLD
        }
        #[cfg(target_os = "ios")]
        {
            let _ = window;
            gpui_ios::keyboard_height() > 1.0
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = window;
            false
        }
    }

    pub fn render_home_content(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input_focused = self.input_focus.is_focused(window);
        let ime_visible = Self::home_ime_visible(window);
        #[cfg(target_os = "android")]
        if input_focused && !self.last_home_input_focus {
            self.home_input_rotation_line_cap = None;
        }
        #[cfg(target_os = "android")]
        if ime_visible && !self.last_home_ime_visible {
            // Tapping the still-focused editor after the keyboard was hidden
            // starts a new input session even though EditText never lost
            // focus. Release the rotation height lock for that transition.
            self.home_input_rotation_line_cap = None;
        }
        self.last_home_ime_visible = ime_visible;

        #[cfg(target_os = "android")]
        let viewport_width: f32 = window.fully_visible_bounds().size.width.into();
        #[cfg(target_os = "ios")]
        let window_height: f32 = window.viewport_size().height.into();
        #[cfg(target_os = "android")]
        let window_height: f32 = window.viewport_size().height.into();
        #[cfg(not(target_os = "android"))]
        let viewport_width: f32 = window.viewport_size().width.into();
        let visible_height: f32 = window.fully_visible_bounds().size.height.into();
        let height_changed = (visible_height - self.home_input_layout_height).abs() > 0.5;
        let width_changed = if self.home_input_layout_width <= 0.0 {
            self.home_input_layout_width = viewport_width;
            false
        } else if (viewport_width - self.home_input_layout_width).abs() > 0.5 {
            self.home_input_layout_width = viewport_width;
            true
        } else {
            false
        };
        self.home_input_layout_height = visible_height;
        #[cfg(target_os = "ios")]
        if width_changed || height_changed {
            // Rotation invalidates the previous native measurement. Let the
            // portrait layout measure and grow the editor naturally again.
            self.home_input_ime_line_cap = None;
            self.home_native_input_lines = None;
        }
        #[cfg(target_os = "android")]
        if width_changed || height_changed {
            // The input-height limit belongs to the current window size.
            // Recompute it after rotating back from landscape to portrait
            // instead of carrying the previous orientation's cap forward.
            self.home_input_ime_line_cap = None;
            self.home_input_rotation_line_cap = None;
        }

        let compact_max_lines = self.compact_home_input_lines(window);
        #[cfg(target_os = "android")]
        let normal_max_lines = android_max_lines_for_window_height(window_height);
        #[cfg(target_os = "ios")]
        let normal_max_lines = ios_max_lines_for_window_height(window_height);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let normal_max_lines = normal_max_lines_for_height(visible_height, 0.0);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let observed_ime_cap: Option<usize> = if ime_visible {
            compact_max_lines.or(Some(normal_max_lines))
        } else {
            None
        };
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Some(cap) = observed_ime_cap {
            self.home_input_ime_line_cap = Some(
                self.home_input_ime_line_cap
                    .map_or(cap, |previous| previous.max(cap)),
            );
        }
        #[cfg(target_os = "android")]
        {
            self.home_input_ime_line_cap = None;
            self.home_input_rotation_line_cap = None;
        }
        #[cfg(target_os = "android")]
        let input_line_cap = normal_max_lines;
        #[cfg(target_os = "ios")]
        let input_line_cap = normal_max_lines;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let input_line_cap = self.home_input_ime_line_cap.unwrap_or(normal_max_lines);
        self.home_input_effective_line_cap = input_line_cap;
        // Native EditText line ranges arrive one frame after the text does.
        // Deriving the container height from that delayed snapshot makes paste
        // growth late and makes every held Backspace collapse then regrow.
        // Shape the current value synchronously instead; native ranges remain
        // authoritative for caret/selection mapping.
        let input_viewport_width: f32 = self.input_scroll.bounds().size.width.into();
        let wrap_width = if width_changed || input_viewport_width <= 0.0 {
            viewport_width - 58.0
        } else {
            input_viewport_width - 26.0
        };
        let content_lines = {
            #[cfg(target_os = "ios")]
            {
                self.home_native_input_lines.unwrap_or_else(|| {
                    input_visual_line_count(window, &self.input, wrap_width, palette)
                })
            }
            #[cfg(not(target_os = "ios"))]
            {
                let _ = wrap_width;
                input_visual_line_count(window, &self.input, wrap_width, palette)
            }
        };
        let input_overflows = content_lines > input_line_cap;
        let base_lines = DEFAULT_HOME_INPUT_LINES.min(input_line_cap);
        // The persisted IME cap protects the layout after the keyboard closes.
        // Content height itself is still allowed to grow and shrink directly
        // with the current text, so Clear and deleting wrapped rows recover
        // immediately without an animation or delayed native callback.
        let input_lines = content_lines.max(base_lines).min(input_line_cap);
        #[cfg(target_os = "ios")]
        let input_chrome_height = IOS_INPUT_CHROME_HEIGHT;
        #[cfg(not(target_os = "ios"))]
        let input_chrome_height = COMPACT_INPUT_CHROME_HEIGHT;
        #[cfg(target_os = "ios")]
        let min_input_height = DEFAULT_HOME_INPUT_LINES.min(input_line_cap) as f32
            * MULTILINE_LINE_HEIGHT
            + input_chrome_height;
        #[cfg(not(target_os = "ios"))]
        let min_input_height = input_lines as f32 * MULTILINE_LINE_HEIGHT + input_chrome_height;
        let max_input_height = input_line_cap as f32 * MULTILINE_LINE_HEIGHT + input_chrome_height;
        // Keep the native text renderer mounted for the whole page
        // transition. Switching back to GPUI text while the page slides makes
        // iOS wrap the same string differently and makes Android flash its
        // synthetic scrollbar. The editor is hidden after a transition only
        // when the destination is no longer Home.
        let home_in_layout = self.route == crate::Route::Home
            || self.transition.as_ref().is_some_and(|transition| {
                transition.from == crate::Route::Home || transition.to == crate::Route::Home
            });
        // Android has exactly one visible input surface: the system EditText.
        // GPUI only reserves the layout slot. Toggling the native surface on
        // focus/content changed the visual renderer during Paste/Clear and
        // briefly showed both borders at once. Keep the native surface mounted
        // for the whole Home page and let Rust update its theme, text and
        // bounds. Other platforms still use their existing handoff rules.
        // Mobile fields have exactly one visible renderer: the platform's
        // UITextView/EditText. GPUI reserves a fully transparent slot. Do not
        // switch back to its painted field while the value is empty or focus
        // is elsewhere; that produces two borders during a single frame.
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let native_editor_enabled = home_in_layout && self.dialog.is_none();
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let native_editor_enabled = home_in_layout && self.dialog.is_none();
        #[cfg(target_os = "android")]
        let native_editor_clip_bounds = self
            .transition
            .as_ref()
            .filter(|transition| {
                transition.from == crate::Route::Home || transition.to == crate::Route::Home
            })
            .map(|transition| {
                // 裁剪几何必须与转场滑动几何完全一致：滑动层是 root 的
                // safe-area padding 之内 inset_0 的图层，宽度 = viewport
                // 减左右 padding。fully_visible_bounds 在刘海设备/转场
                // 期间与该几何不一致且数值会瞬变，会让编辑器多露出
                // 一条约等于刘海宽度的未覆盖条带。
                let (safe_left, safe_top, safe_right, safe_bottom) =
                    gpui_android::safe_area_insets_logical();
                let viewport = window.viewport_size();
                let viewport_width: f32 = f32::from(viewport.width);
                let viewport_height: f32 = f32::from(viewport.height);
                // 刘海补偿只用于横屏逆时针旋转（ROTATION_90，刘海在左侧）。
                // 竖屏和横屏顺时针（ROTATION_270，刘海在右侧）固定为 0。
                let portrait = viewport_height >= viewport_width;
                let compensate_cutout = !portrait && gpui_android::display_rotation() == 1;
                let (comp_left, comp_top, comp_bottom) = if compensate_cutout {
                    (safe_left, safe_top, safe_bottom)
                } else {
                    (0.0, 0.0, 0.0)
                };
                let stage_left: f32 = safe_left;
                let stage_right = (viewport_width - safe_right).max(stage_left);
                // 与滑动动画共用同一个屏幕宽度偏移量，clip 右边界就是
                // 移动页当前左缘；页还完全在屏幕外时夹到 stage 右缘。
                let slide_offset = crate::transition_slide_offset(
                    viewport_width,
                    transition.kind,
                    transition.progress,
                );
                let moving_page_left = (stage_left + slide_offset).clamp(stage_left, stage_right);
                let clip_right = moving_page_left;
                let clip_left = comp_left;
                let clip_top = comp_top;
                let clip_width = (clip_right - clip_left).max(0.0);
                let clip_height = (viewport_height - comp_top - comp_bottom).max(0.0);
                (clip_left, clip_top, clip_width, clip_height)
            });
        #[cfg(target_os = "ios")]
        let native_editor_clip_bounds = self
            .transition
            .as_ref()
            .filter(|transition| {
                transition.from == crate::Route::Home || transition.to == crate::Route::Home
            })
            .map(|transition| {
                // On iOS the native text view is a subview of the same window
                // as the GPUI stage. The stage starts below the root's safe
                // area and its width is the window width minus left/right
                // safe-area padding. There is no Android-style cutout
                // correction: the landscape cutout inset is zero here.
                // Match the moving page's actual `ml(relative(slide))`
                // geometry in window coordinates frame by frame.
                let (safe_top, safe_left, safe_bottom, safe_right) = gpui_ios::safe_area_insets();
                // The Android landscape cutout correction must not be applied
                // on iPhone. Keep the cutout contribution to the clip at zero.
                let cutout_inset = 0.0_f32;
                let viewport_width = f32::from(window.viewport_size().width);
                let viewport_height = f32::from(window.viewport_size().height);
                let stage_left = safe_left - cutout_inset;
                let stage_top = safe_top;
                let stage_width =
                    (viewport_width - safe_left - safe_right - cutout_inset * 2.0).max(0.0);
                let stage_height = (viewport_height - safe_top - safe_bottom).max(0.0);
                // 与滑动页共用同一个屏幕宽度偏移量：移动页左缘 =
                // stage 左缘 + slide_offset，clip 右缘与其逐帧一致。
                // 完全移出时夹到 stage 右缘，避免负宽度。
                let slide_offset = crate::transition_slide_offset(
                    viewport_width,
                    transition.kind,
                    transition.progress,
                );
                let stage_right = stage_left + stage_width;
                let moving_page_left = (stage_left + slide_offset).clamp(stage_left, stage_right);
                (
                    stage_left,
                    stage_top,
                    moving_page_left - stage_left,
                    stage_height,
                )
            });
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let native_editor_clip_bounds = None;
        if !is_desktop() {
            self.schedule_home_input_reveal(window, compact_max_lines.is_some());
        }
        #[cfg(target_os = "ios")]
        if !home_in_layout || self.dialog.is_some() {
            crate::ios_native_input::hide_placeholder("url-input");
        }
        // Compact mode shows only the editor, so landscape keeps it flush
        // with the top safe area instead of leaving the column's 16px gap.
        #[cfg(target_os = "android")]
        let compact_landscape = compact_max_lines.is_some() && viewport_width > visible_height;
        #[cfg(not(target_os = "android"))]
        let compact_landscape = false;
        let mut column = div().w_full().flex().flex_col();
        column = column.px(px(16.0)).pb(px(16.0));
        if !compact_landscape {
            column = column.pt(px(16.0));
        }
        column = column.child(self.render_input_card(
            palette,
            max_input_height,
            input_lines,
            min_input_height,
            input_focused,
            native_editor_enabled,
            native_editor_clip_bounds,
            input_overflows,
            compact_max_lines.is_none(),
            cx,
        ));

        if self.loading && compact_max_lines.is_none() {
            column = column.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(24.0))
                    .gap(px(16.0))
                    .child(widgets::Spinner {
                        phase: self.spinner_phase,
                        color: palette.primary,
                        size: 36.0,
                    })
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(palette.text_secondary)
                            .child("正在解析链接..."),
                    ),
            );
        }

        column.into_any_element()
    }

    fn schedule_home_input_reveal(&mut self, window: &mut Window, align_input_top: bool) {
        let input_focused = self.input_focus.is_focused(window);
        let focus_lost = !input_focused && self.last_home_input_focus;
        let visible_bottom = f32::from(window.fully_visible_bounds().bottom());
        let visible_bottom_changed = (visible_bottom - self.last_visible_bottom).abs() > 0.5;
        let focus_gained = input_focused && !self.last_home_input_focus;
        self.last_visible_bottom = visible_bottom;
        self.last_home_input_focus = input_focused;

        if focus_lost {
            self.home_scroll
                .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
            window.refresh();
            return;
        }

        if !input_focused || !(visible_bottom_changed || focus_gained) {
            return;
        }

        let input_scroll = self.input_scroll.clone();
        let home_scroll = self.home_scroll.clone();
        window.on_next_frame(move |window, _| {
            let max_offset = f32::from(home_scroll.max_offset().y).max(0.0);
            let current = f32::from(home_scroll.offset().y);
            let target = if align_input_top {
                let visible_top = window.fully_visible_bounds().origin.y;
                let overflow = f32::from(input_scroll.bounds().top() - visible_top);
                if overflow <= 0.5 {
                    return;
                }
                (current - overflow).clamp(-max_offset, 0.0)
            } else {
                let overflow =
                    f32::from(input_scroll.bounds().bottom() - home_scroll.bounds().bottom());
                if overflow <= 0.5 {
                    return;
                }
                (current - overflow).clamp(-max_offset, 0.0)
            };
            if (target - current).abs() > 0.5 {
                home_scroll.set_offset(gpui::point(gpui::px(0.0), gpui::px(target)));
                window.refresh();
            }
        });
    }

    fn render_input_card(
        &mut self,
        palette: Palette,
        max_input_height: f32,
        input_lines: usize,
        min_input_height: f32,
        input_focused: bool,
        native_editor_enabled: bool,
        native_editor_clip_bounds: Option<(f32, f32, f32, f32)>,
        input_overflows: bool,
        show_actions: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_content = !self.input.is_empty();
        let entity = cx.entity();
        #[cfg(not(target_os = "ios"))]
        let text_entity = entity.clone();
        let paste_entity = entity.clone();
        let clear_entity = entity.clone();
        let parse_entity = entity.clone();

        // GPUI's full Input widget is still the renderer on desktop and is
        // Android's transparent geometry bridge. On iOS the tree contains
        // only an empty div: Swift owns every visible input pixel.
        #[cfg(not(target_os = "ios"))]
        let input_container = {
            let input = Input::new("url-input")
                .focus_handle(self.input_focus.clone())
                .multiline(true)
                .min_lines(input_lines)
                .native_editor_min_height(min_input_height)
                .native_editor_enabled(native_editor_enabled)
                .native_editor_clip_bounds(native_editor_clip_bounds)
                .editor_viewport_height(max_input_height)
                .editor_scroll_handle(self.input_scroll.clone())
                .value(self.input.clone())
                .placeholder("输入或粘贴短视频分享链接，支持多链接解析")
                .size(InputSize::Lg)
                .select_all_on_ctrl(cfg!(target_os = "windows"))
                .bg_color(palette.bg)
                .border_color(if is_desktop() {
                    gpui::rgba(0x00000000)
                } else {
                    palette.input_border
                })
                .text_color(palette.text)
                .placeholder_color(palette.hint)
                .on_text_change(move |text, _, cx| {
                    text_entity.update(cx, |this, cx| {
                        if this.input != text {
                            this.input = text.to_string();
                            cx.notify();
                        }
                    });
                });
            div().relative().w_full().child(input)
        };

        #[cfg(target_os = "ios")]
        let input_slot_height = (input_lines as f32 * MULTILINE_LINE_HEIGHT
            + IOS_INPUT_CHROME_HEIGHT)
            .max(min_input_height)
            .min(max_input_height);
        #[cfg(target_os = "ios")]
        let input_container = {
            let palette = palette;
            let max_input_height = max_input_height;
            let min_input_height = min_input_height;
            let native_editor_clip_bounds = native_editor_clip_bounds;
            let placeholder_entity = entity.clone();
            div()
                .relative()
                .w_full()
                .on_children_prepainted({
                    move |bounds, window, cx| {
                        let Some(bounds) = bounds.first() else {
                            return;
                        };
                        let (x, y, width, height) = (
                            f32::from(bounds.origin.x),
                            f32::from(bounds.origin.y),
                            f32::from(bounds.size.width),
                            f32::from(bounds.size.height),
                        );
                        placeholder_entity.update(cx, |view, cx| {
                            crate::ios_native_input::show_placeholder(
                                "url-input",
                                &view.input,
                                x,
                                y,
                                width,
                                height,
                                max_input_height,
                                min_input_height,
                                native_editor_clip_bounds,
                                palette,
                                view.input_focus.is_focused(window),
                                false,
                            );
                        });
                    }
                })
                .h(px(input_slot_height))
                .child(
                    div()
                        .id("url-input-placeholder")
                        .relative()
                        .w_full()
                        .h_full()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            crate::ios_native_input::focus("url-input");
                            cx.stop_propagation();
                        }),
                )
        };
        #[cfg(target_os = "ios")]
        let input_scroll = div()
            .id("url-input-scroll")
            .relative()
            .w_full()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                crate::ios_native_input::focus("url-input");
                cx.stop_propagation();
            })
            .child(input_container);

        #[cfg(not(target_os = "ios"))]
        let scrollbar_width = if cfg!(not(any(target_os = "android", target_os = "ios"))) {
            0.0
        } else {
            8.0
        };
        #[cfg(not(target_os = "ios"))]
        let mut input_scroll = div()
            .id("url-input-scroll")
            .w_full()
            .max_h(px(max_input_height))
            .overflow_y_scroll()
            .scrollbar_width(px(scrollbar_width))
            .track_scroll(&self.input_scroll)
            .text_size(px(16.0))
            .child(input_container);

        #[cfg(target_os = "android")]
        if native_editor_enabled {
            input_scroll = input_scroll.overflow_hidden().scrollbar_width(px(0.0));
        }

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let input_scroll = input_scroll.on_mouse_up_out(MouseButton::Left, {
            let focus_handle = self.input_focus.clone();
            let input_bounds = self.input_scroll.bounds();
            move |_, window, cx| {
                if focus_handle.is_focused(window) {
                    window.dispatch_event(
                        PlatformInput::MouseUp(MouseUpEvent {
                            button: MouseButton::Left,
                            position: input_bounds.center(),
                            modifiers: Modifiers::default(),
                            click_count: 1,
                        }),
                        cx,
                    );
                }
            }
        });

        let scrollbar = if cfg!(any(target_os = "android", target_os = "ios")) || !input_overflows {
            div().into_any_element()
        } else {
            widgets::vertical_scrollbar(
                cx,
                "home-input-scrollbar",
                widgets::ScrollbarTarget::HomeInput,
                self.input_scrollbar_drag.clone(),
                self.input_scroll.clone(),
                palette,
            )
        };

        let mut input_box = div()
            .relative()
            .w_full()
            .max_h(px(max_input_height))
            .child(input_scroll)
            .child(scrollbar);

        #[cfg(not(target_os = "ios"))]
        {
            if !native_editor_enabled {
                input_box = input_box.overflow_hidden().rounded_md();
            }
            let scroll_metrics_entity = entity;
            let scroll_metrics_handle = self.input_scroll.clone();
            input_box = input_box.on_children_prepainted(move |_, _, cx| {
                let max = f32::from(scroll_metrics_handle.max_offset().y);
                let viewport = f32::from(scroll_metrics_handle.bounds().size.height);
                scroll_metrics_entity.update(cx, |this, cx| {
                    if this.input_scroll_metrics != (max, viewport) {
                        this.input_scroll_metrics = (max, viewport);
                        cx.notify();
                    }
                });
            });
        }

        let draw_gpui_input_chrome =
            !cfg!(any(target_os = "android", target_os = "ios")) || !native_editor_enabled;
        if draw_gpui_input_chrome {
            let border_color = if input_focused {
                palette.input_focus
            } else {
                palette.input_border
            };
            input_box = input_box.bg(palette.bg).child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_md()
                    .border_1()
                    .border_color(border_color),
            );
        }
        let buttons = div()
            .mt(px(16.0))
            .w_full()
            .flex()
            .gap(px(12.0))
            .child({
                let paste_button = div().flex_1();
                paste_button.child(
                    ActionButton::new("paste-button", "粘贴", palette.paste_button())
                        .icon(icons::PASTE, 16.0)
                        .on_click(move |_, _, cx| {
                            paste_entity.update(cx, |this, cx| this.paste_from_clipboard(cx));
                        }),
                )
            })
            .child({
                let clear_on_press_entity = clear_entity.clone();
                let clear_button = div().flex_1();
                let clear_button = if has_content {
                    clear_button.on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                        clear_on_press_entity.update(cx, |this, cx| this.clear_input(cx));
                    })
                } else {
                    clear_button
                };
                let clear_button = clear_button.child(
                    ActionButton::new("clear-button", "清空", palette.clear_button(has_content))
                        .icon(icons::REFRESH, 20.0)
                        .enabled(has_content)
                        .on_click(move |_, _, cx| {
                            clear_entity.update(cx, |this, cx| this.clear_input(cx));
                        }),
                );
                clear_button
            })
            .child(
                div().flex_1().child(
                    ActionButton::new("parse-button", "解析", palette.parse_button(has_content))
                        .icon(icons::SEND, 16.0)
                        .enabled(has_content)
                        .on_click(move |_, window, cx| {
                            parse_entity.update(cx, |this, cx| this.parse(window, cx));
                        }),
                ),
            );

        if show_actions {
            div()
                .w_full()
                .flex()
                .flex_col()
                .px(px(16.0))
                .pb(px(16.0))
                .child(input_box)
                .child(buttons)
                .into_any_element()
        } else {
            div().w_full().child(input_box).into_any_element()
        }
    }

    pub fn close_results(&mut self, cx: &mut Context<Self>) {
        self.navigate_back(cx);
    }

    /// macOS 导出脚本按钮的悬浮提示，三个下载器名称可点击打开浏览器。
    #[cfg(target_os = "macos")]
    fn render_export_script_tooltip(&self, palette: Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("export-script-tooltip")
            .absolute()
            .left(px(0.0))
            .bottom_full()
            .mb(px(8.0))
            .occlude()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(8.0))
            .bg(palette.toast_bg)
            .border_1()
            .border_color(palette.divider)
            .shadow_md()
            .flex()
            .items_center()
            .whitespace_nowrap()
            .text_size(px(12.0))
            .line_height(px(18.0))
            .text_color(palette.text_secondary)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.set_export_tooltip_hover(*hovered, cx)
            }))
            .child("支持 ")
            .child(crate::dialogs::link_text(
                "export-tooltip-aria2c",
                "aria2c",
                crate::dialogs::ARIA2C_URL,
                palette,
            ))
            .child("、")
            .child(crate::dialogs::link_text(
                "export-tooltip-downloader",
                "downloader",
                crate::dialogs::DOWNLOADER_URL,
                palette,
            ))
            .child("、")
            .child(crate::dialogs::link_text(
                "export-tooltip-robust-downloader",
                "RobustDownloader",
                crate::dialogs::ROBUST_DOWNLOADER_URL,
                palette,
            ))
            .child(" 下载脚本")
            .into_any_element()
    }

    /// 结果独占一页：列表滚动区自动占满窗口剩余高度。
    pub fn render_results_page(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.result_view == crate::ResultViewMode::Simple {
            return self.render_results_page_experience(palette, window, cx);
        }
        let count: usize = self.results.iter().map(ResultItem::entry_count).sum();
        let viewport_width = f32::from(window.bounds().size.width);
        let title_wrap_width = (viewport_width - 136.0).max(120.0);
        let mut items = Vec::new();
        for (index, item) in self.results.iter().enumerate() {
            let kind_label = match item.media_kind {
                MediaKind::Video => "视频",
                MediaKind::Image => "图片",
            };
            let kind_color = match item.media_kind {
                MediaKind::Video => crate::theme::PRIMARY,
                MediaKind::Image => 0xD12CFE,
            };
            let mut cards: Vec<AnyElement> = Vec::new();
            for (entry_index, entry) in item.entries.iter().enumerate() {
                let group: SharedString = format!("result-entry-{index}-{entry_index}").into();
                let entry_entity = cx.entity();
                let meta = self.entry_meta(entry);
                let idle_hint = div()
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(palette.text_muted)
                    .whitespace_nowrap()
                    .group_hover(group.clone(), |style| style.opacity(0.0))
                    .child("点击链接复制");
                let hover_hint = div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .opacity(0.0)
                    .group_hover(group.clone(), |style| style.opacity(1.0))
                    .child(widgets::icon(icons::LINK, 13.0, palette.primary))
                    .child(
                        div()
                            .text_color(palette.primary)
                            .whitespace_nowrap()
                            .child("复制链接"),
                    );
                cards.push(
                    div()
                        .id(format!("result-entry-{index}-{entry_index}"))
                        .group(group)
                        .mb(px(8.0))
                        .p(px(12.0))
                        .w_full()
                        .min_w(px(0.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(gpui::rgba(0x00000000))
                        .bg(palette.result_item_bg)
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .cursor_pointer()
                        .hover(|style| style.border_color(palette.primary))
                        .on_click(move |_, _, cx| {
                            entry_entity
                                .update(cx, |this, cx| this.copy_entry(index, entry_index, cx));
                        })
                        .child(
                            div()
                                .w_full()
                                .min_w(px(0.0))
                                .font_family(crate::theme::mono_font())
                                .text_size(px(13.0))
                                .line_height(px(20.0))
                                .text_color(palette.primary)
                                .underline()
                                .text_decoration_color(palette.primary)
                                .text_decoration_solid()
                                .whitespace_normal()
                                .child(entry.url.clone()),
                        )
                        .child(
                            div()
                                .w_full()
                                .min_w(px(0.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .text_size(px(12.0))
                                        .line_height(px(18.0))
                                        .text_color(palette.text_muted)
                                        .whitespace_normal()
                                        .child(meta),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .flex_shrink_0()
                                        .child(idle_hint)
                                        .child(hover_hint),
                                ),
                        )
                        .into_any_element(),
                );
            }
            let kind_badge = div()
                .flex_shrink_0()
                .px(px(6.0))
                .rounded(px(4.0))
                .text_size(px(11.0))
                .line_height(px(18.0))
                .text_color(gpui::rgb(kind_color))
                .border_1()
                .border_color(gpui::rgb(kind_color))
                .child(kind_label);
            let reserved_width = title_meta_width(window, kind_label, "", palette);
            let title_lines = wrap_title_lines(window, &item.title, title_wrap_width, palette);
            let last_title_line = title_lines.len().saturating_sub(1);
            let mut header = div().flex().flex_col().w_full().min_w(px(0.0));
            for line in title_lines.iter().take(last_title_line) {
                header = header.child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .font_family(crate::theme::mono_font())
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .text_color(palette.text)
                        .whitespace_nowrap()
                        .child(line.text.clone()),
                );
            }
            let last_title = title_lines
                .get(last_title_line)
                .map(|line| line.text.clone())
                .unwrap_or_default();
            let last_title_width = title_lines
                .get(last_title_line)
                .map(|line| line.width)
                .unwrap_or(0.0);
            let meta_fits_on_last_line =
                last_title_width + reserved_width <= title_wrap_width + 2.0;
            let badge_row = div().flex().items_center().gap(px(8.0)).child(kind_badge);
            if meta_fits_on_last_line {
                header = header.child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(8.0))
                        .w_full()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .flex_shrink_0()
                                .font_family(crate::theme::mono_font())
                                .text_size(px(12.0))
                                .line_height(px(18.0))
                                .text_color(palette.text)
                                .whitespace_nowrap()
                                .child(last_title),
                        )
                        .child(badge_row),
                );
            } else {
                header = header.child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .font_family(crate::theme::mono_font())
                        .text_size(px(12.0))
                        .line_height(px(18.0))
                        .text_color(palette.text)
                        .whitespace_nowrap()
                        .child(last_title),
                );
                header = header.child(div().w_full().flex().child(badge_row));
            }
            items.push(
                div()
                    .mb(px(18.0))
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(24.0))
                                    .h(px(24.0))
                                    .rounded_full()
                                    .bg(gpui::rgba(0x3586f91a))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(palette.primary)
                                    .child(format!("{}", index + 1)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .child(header),
                            ),
                    )
                    .children(cards)
                    .into_any_element(),
            );
        }

        let copy_entity = cx.entity();
        let export_entity = cx.entity();
        let download_entity = cx.entity();

        let mut action_row = div().mt(px(16.0)).w_full().flex().gap(px(12.0)).child(
            div().flex_1().child(
                ActionButton::new(
                    "copy-all-button",
                    format!("复制链接 ({count})"),
                    palette.copy_button(),
                )
                .icon(icons::COPY, 16.0)
                .on_click(move |_, _, cx| {
                    copy_entity.update(cx, |this, cx| this.copy_all(cx));
                }),
            ),
        );

        if is_desktop() {
            #[cfg(target_os = "windows")]
            let export_label = "导出与下载";
            #[cfg(not(target_os = "windows"))]
            let export_label = "导出下载脚本";
            let mut export_cell = div().relative().flex_1().id("export-button-hover");
            #[cfg(target_os = "macos")]
            {
                export_cell = export_cell.on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    this.set_export_tooltip_hover(*hovered, cx)
                }));
            }
            export_cell = export_cell.child(
                ActionButton::new("export-button", export_label, palette.export_button())
                    .icon(icons::TERMINAL, 16.0)
                    .on_click(move |_, _, cx| {
                        export_entity.update(cx, |this, cx| this.export_scripts(cx));
                    }),
            );
            #[cfg(target_os = "macos")]
            if self.export_tooltip_visible {
                export_cell = export_cell.child(self.render_export_script_tooltip(palette, cx));
            }
            action_row = action_row.child(export_cell);
        }

        action_row = action_row.child(
            div().flex_1().child(
                ActionButton::new(
                    "download-button",
                    "下载资源".to_string(),
                    palette.download_button(),
                )
                .icon(icons::DOWNLOAD, 16.0)
                .on_click(move |_, _, cx| {
                    download_entity.update(cx, |this, cx| this.request_download(cx));
                }),
            ),
        );

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .px(px(16.0))
            .pt(px(8.0))
            .pb(px(16.0))
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.0))
                    .id("results-scroll")
                    .overflow_y_scroll()
                    .track_scroll(&self.results_scroll)
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .bg(palette.result_bg)
                    .child(div().w_full().flex().flex_col().children(items)),
            )
            .child(action_row)
            .into_any_element()
    }
}

pub fn format_progress_size(value: u64) -> String {
    format_size(value)
}
