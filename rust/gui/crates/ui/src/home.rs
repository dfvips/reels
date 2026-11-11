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

use douyin_core::extractor::{MediaEntry, MediaKind, MediaPlatform, VideoExtractor};
use douyin_input::{Input, InputSize};
use gpui::{
    div, px, AnyElement, ClipboardItem, Context, FontWeight, InteractiveElement, IntoElement,
    Modifiers, MouseButton, MouseUpEvent, ParentElement, PlatformInput, SharedString,
    StatefulInteractiveElement, Styled, TextRun, WeakEntity, Window,
};

use crate::paths::{format_size, is_desktop};
use crate::theme::Palette;
use crate::widgets::{self, ActionButton};
use crate::{icons, AppView, ResultItem, ToastKind};

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
    /// 标题行摘要：视频显示时长 / 大小 / 清晰度，图片显示大小 / 分辨率。
    fn entry_summary(&self, entry: Option<&MediaEntry>) -> String {
        let Some(entry) = entry else {
            return String::new();
        };
        let mut parts: Vec<String> = Vec::new();
        if let Some(duration) = entry.duration_secs {
            parts.push(format!("时长 {}", format_duration(duration)));
        }
        if let Some(size) = entry.size_bytes {
            parts.push(format!("大小 {}", format_size(size)));
        }
        if let Some(quality) = &entry.quality {
            parts.push(quality.clone());
        }
        // RAW 条目没有清晰度档位名称，识别出的分辨率也要展示。
        if entry.quality.as_deref().map(|q| q == "RAW").unwrap_or(true) {
            if let (Some(width), Some(height)) = (entry.width, entry.height) {
                parts.push(format!("{width}x{height}"));
            }
        }
        parts.join(" · ")
    }

    /// 多行链接时每行的行内元数据。
    fn entry_line_detail(&self, entry: &MediaEntry) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(quality) = &entry.quality {
            parts.push(quality.clone());
        }
        if let Some(size) = entry.size_bytes {
            parts.push(format_size(size));
        }
        if entry.quality.is_none() {
            if let (Some(width), Some(height)) = (entry.width, entry.height) {
                parts.push(format!("{width}x{height}"));
            }
        } else if let (Some(width), Some(height)) = (entry.width, entry.height) {
            parts.push(format!("{width}x{height}"));
        }
        parts.join(" · ")
    }

    pub fn parse(&mut self, cx: &mut Context<Self>) {
        let text = self.input.clone();
        if text.trim().is_empty() {
            self.toast("请输入抖音视频链接", ToastKind::Error, cx);
            return;
        }
        self.loading = true;
        self.results.clear();
        self.dialog = None;
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
                            MediaPlatform::Douyin => format!("抖音视频_{}", items.len() + 1),
                            MediaPlatform::Xhs => format!("小红书笔记_{}", items.len() + 1),
                        });
                    items.push(ResultItem {
                        title,
                        platform: result.platform,
                        media_kind: result.media_kind,
                        entries: result.entries,
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
        self.toast(
            format!("成功解析 {count} 个链接"),
            ToastKind::Success,
            cx,
        );
        self.route = crate::Route::Results;
    }

    pub fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                self.input = text;
                cx.notify();
            }
        }
    }

    pub fn clear_input(&mut self, cx: &mut Context<Self>) {
        self.input.clear();
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

    pub fn copy_single(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(item) = self.results.get(index) else {
            return;
        };
        let mut lines = vec![item.title.clone()];
        lines.extend(item.entries.iter().map(|entry| entry.url.clone()));
        let text = lines.join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.toast("已复制到剪贴板", ToastKind::Info, cx);
    }

    pub fn export_scripts(&mut self, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            self.toast("没有可导出的内容", ToastKind::Error, cx);
            return;
        }
        if !self.settings.last_directory.is_empty() {
            let directory = PathBuf::from(&self.settings.last_directory);
            self.export_scripts_to(directory, cx);
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
                    this.export_scripts_to(directory, cx);
                }
                None => this.toast("导出已取消", ToastKind::Info, cx),
            });
        })
        .detach();
    }

    fn export_scripts_to(&mut self, directory: PathBuf, cx: &mut Context<Self>) {
        if let Err(error) = std::fs::create_dir_all(&directory) {
            self.toast(format!("导出下载脚本失败: {error}"), ToastKind::Error, cx);
            return;
        }
        let entries: Vec<douyin_core::script::ScriptEntry> = self
            .results
            .iter()
            .flat_map(|item| {
                item.entries
                    .iter()
                    .map(move |entry| douyin_core::script::ScriptEntry {
                        title: item.title.clone(),
                        url: entry.url.clone(),
                        extension: entry.extension.clone(),
                    })
            })
            .collect();
        let scripts = douyin_core::script::build_scripts(
            &entries,
            cfg!(target_os = "windows"),
        );
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
        let height: f32 = window.bounds().size.height.into();
        let max_input_height = height / 3.0;
        let input_focused = self.input_focus.is_focused(window);

        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .p(px(16.0))
            .child(self.render_input_card(palette, max_input_height, input_focused, cx));

        if self.loading {
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

        div()
            .id("home-scroll")
            .flex_1()
            .w_full()
            .overflow_y_scroll()
            .child(column)
            .into_any_element()
    }

    fn render_input_card(
        &mut self,
        palette: Palette,
        max_input_height: f32,
        input_focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity();
        let scroll_metrics_entity = entity.clone();
        let scroll_metrics_handle = self.input_scroll.clone();
        let has_content = !self.input.is_empty();
        let text_entity = entity.clone();
        let paste_entity = entity.clone();
        let clear_entity = entity.clone();
        let parse_entity = entity.clone();

        let input = Input::new("url-input")
            .focus_handle(self.input_focus.clone())
            .multiline(true)
            .min_lines(6)
            .value(self.input.clone())
            .placeholder("输入或粘贴抖音视频分享链接")
            .size(InputSize::Lg)
            .bg_color(palette.bg)
            .border_color(gpui::rgba(0x00000000))
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

        let input_with_border_mask = div().relative().w_full().child(input).child(
            div()
                .absolute()
                .inset_0()
                .rounded_md()
                .border_1()
                .border_color(palette.bg),
        );

        let border_color = if input_focused {
            palette.input_focus
        } else {
            palette.input_border
        };

        let input_scroll = div()
            .id("url-input-scroll")
            .w_full()
            .max_h(px(max_input_height))
            .overflow_y_scroll()
            .scrollbar_width(px(8.0))
            .track_scroll(&self.input_scroll)
            .text_size(px(16.0))
            .child(input_with_border_mask)
            .on_mouse_up_out(MouseButton::Left, {
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

        let scroll_max = f32::from(self.input_scroll.max_offset().y);
        let scroll_viewport = f32::from(self.input_scroll.bounds().size.height);
        let scroll_offset = f32::from(-self.input_scroll.offset().y);
        let scrollbar = if scroll_max > 0.0 && scroll_viewport > 0.0 {
            let thumb_height = (scroll_viewport * scroll_viewport / (scroll_viewport + scroll_max))
                .clamp(28.0, scroll_viewport);
            let thumb_travel = (scroll_viewport - thumb_height).max(0.0);
            let thumb_top = (scroll_offset / scroll_max).clamp(0.0, 1.0) * thumb_travel;
            div()
                .absolute()
                .top(px(thumb_top + 6.0))
                .right(px(4.0))
                .w(px(4.0))
                .h(px((thumb_height - 12.0).max(4.0)))
                .rounded_full()
                .bg(palette.text_muted.opacity(0.75))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let input_box = div()
            .relative()
            .w_full()
            .max_h(px(max_input_height))
            .overflow_hidden()
            .rounded_md()
            .bg(palette.bg)
            .on_children_prepainted(move |_, _, cx| {
                let max = f32::from(scroll_metrics_handle.max_offset().y);
                let viewport = f32::from(scroll_metrics_handle.bounds().size.height);
                scroll_metrics_entity.update(cx, |this, cx| {
                    if this.input_scroll_metrics != (max, viewport) {
                        this.input_scroll_metrics = (max, viewport);
                        cx.notify();
                    }
                });
            })
            .child(input_scroll)
            .child(scrollbar)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_md()
                    .border_1()
                    .border_color(border_color),
            );

        let buttons = div()
            .mt(px(16.0))
            .w_full()
            .flex()
            .gap(px(12.0))
            .child(
                div().flex_1().child(
                    ActionButton::new("paste-button", "粘贴", palette.paste_button())
                        .icon(icons::PASTE, 16.0)
                        .on_click(move |_, _, cx| {
                            paste_entity.update(cx, |this, cx| this.paste_from_clipboard(cx));
                        }),
                ),
            )
            .child(
                div().flex_1().child(
                    ActionButton::new("clear-button", "清空", palette.clear_button(has_content))
                        .icon(icons::REFRESH, 20.0)
                        .enabled(has_content)
                        .on_click(move |_, _, cx| {
                            clear_entity.update(cx, |this, cx| this.clear_input(cx));
                        }),
                ),
            )
            .child(
                div().flex_1().child(
                    ActionButton::new("parse-button", "解析", palette.parse_button(has_content))
                        .icon(icons::SEND, 14.0)
                        .enabled(has_content)
                        .on_click(move |_, _, cx| {
                            parse_entity.update(cx, |this, cx| this.parse(cx));
                        }),
                ),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .px(px(16.0))
            .pb(px(16.0))
            .child(input_box)
            .child(buttons)
            .into_any_element()
    }

    pub fn close_results(&mut self, cx: &mut Context<Self>) {
        self.route = crate::Route::Home;
        cx.notify();
    }

    /// 结果独占一页：列表滚动区自动占满窗口剩余高度。
    pub fn render_results_page(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count: usize = self.results.iter().map(ResultItem::entry_count).sum();
        let viewport_width = f32::from(window.bounds().size.width);
        let title_wrap_width = (viewport_width - 136.0).max(120.0);
        let mut items = Vec::new();
        for (index, item) in self.results.iter().enumerate() {
            let entity = cx.entity();
            let kind_label = match item.media_kind {
                MediaKind::Video => "视频",
                MediaKind::Image => "图片",
            };
            let kind_color = match item.media_kind {
                MediaKind::Video => 0x2e7d32,
                MediaKind::Image => 0x9c27b0,
            };
            let summary = self.entry_summary(item.primary_entry());
            let mut lines: Vec<AnyElement> = Vec::new();
            for (entry_index, entry) in item.entries.iter().enumerate() {
                let mut line = div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .w_full()
                    .min_w(px(0.0));
                if item.entries.len() > 1 {
                    line = line.child(
                        div()
                            .font_family(crate::theme::mono_font())
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(palette.text_muted)
                            .child(format!("{}.", entry_index + 1)),
                    );
                }
                let detail = if item.entries.len() > 1 {
                    self.entry_line_detail(entry)
                } else {
                    String::new()
                };
                let mut url_line = div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(crate::theme::mono_font())
                    .text_size(px(12.0))
                    .line_height(px(18.0))
                    .text_color(palette.text)
                    .whitespace_normal()
                    .child(entry.url.clone());
                if !detail.is_empty() {
                    url_line = url_line.child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(18.0))
                            .text_color(palette.text_muted)
                            .ml(px(8.0))
                            .child(detail),
                    );
                }
                line = line.child(url_line);
                lines.push(line.into_any_element());
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
            let reserved_width = title_meta_width(window, kind_label, &summary, palette);
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
            let mut meta = div().flex().items_center().gap(px(8.0)).child(kind_badge);
            if !summary.is_empty() {
                meta = meta.child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .line_height(px(18.0))
                        .text_color(palette.text_muted)
                        .child(summary.clone()),
                );
            }
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
                        .child(meta),
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
                header = header.child(div().w_full().flex().child(meta));
            }
            items.push(
                div()
                    .id(("result-item", index))
                    .mb(px(12.0))
                    .p(px(8.0))
                    .rounded(px(6.0))
                    .bg(palette.result_item_bg)
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, cx| this.copy_single(index, cx));
                    })
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.0))
                            .w_full()
                            .min_w(px(0.0))
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
                                div().flex_1().min_w(px(0.0)).child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.0))
                                        .w_full()
                                        .min_w(px(0.0))
                                        .child(header)
                                        .children(lines),
                                ),
                            )
                            .child(widgets::icon(icons::COPY, 16.0, palette.primary)),
                    ),
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
            action_row = action_row.child(
                div().flex_1().child(
                    ActionButton::new("export-button", "导出下载脚本", palette.export_button())
                        .icon(icons::TERMINAL, 16.0)
                        .on_click(move |_, _, cx| {
                            export_entity.update(cx, |this, cx| this.export_scripts(cx));
                        }),
                ),
            );
        }

        action_row = action_row.child(
            div().flex_1().child(
                ActionButton::new(
                    "download-button",
                    format!("下载视频 ({count})"),
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

pub fn download_confirm_message(count: usize) -> String {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        format!("视频将按顺序依次下载，下载完成后将保存到应用目录。\n确定要顺序下载这 {count} 个视频吗？")
    } else {
        format!("视频将按顺序依次下载，下载完成后将保存到您选择的位置。\n确定要顺序下载这 {count} 个视频吗？")
    }
}
