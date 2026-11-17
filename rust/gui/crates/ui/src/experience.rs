//! 体验版解析结果页：标签页 + 单条资源预览与操作。

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, px, rgb, rgba, AnyElement, ClipboardItem, Context, InteractiveElement, IntoElement,
    ObjectFit, ParentElement, StatefulInteractiveElement, Styled, StyledImage, WeakEntity, Window,
};

use douyin_core::extractor::{MediaEntry, MediaKind};

use crate::download_flow::{is_audio_entry, is_live_entry};
use crate::paths::{format_size, is_desktop};
use crate::theme::Palette;
use crate::widgets::{self, ActionButton};
use crate::{icons, AppView, ExperienceTab, ResultItem, ResultViewMode, ThumbState, ToastKind};

const PAGE_PADDING: f32 = 16.0;
const TAB_BAR_HEIGHT: f32 = 42.0;
const PREVIEW_PADDING_V: f32 = 12.0;
const ACTION_ROW_TOP: f32 = 12.0;
const ACTION_ROW_BOTTOM: f32 = 16.0;
const ACTION_ROW_HEIGHT: f32 = 40.0;
const NAV_BUTTON_WIDTH: f32 = 64.0;
const ARROW_SLOT_WIDTH: f32 = 72.0;
const SLIDE_SNAP_DURATION: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlideKind {
    Video,
    Image,
    Live,
    Audio,
    Cover,
}

#[derive(Clone)]
struct SlideSpec {
    kind: SlideKind,
    entry_index: usize,
    live_index: Option<usize>,
    /// 封面轮播对应的封面下标。
    cover_index: Option<usize>,
    /// 视频可选清晰度对应的条目下标。
    quality_indices: Vec<usize>,
}

fn video_entry_indices(item: &ResultItem) -> Vec<usize> {
    item.entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            ResultItem::is_video_entry(entry) && !is_audio_entry(entry) && !is_live_entry(entry)
        })
        .map(|(index, _)| index)
        .collect()
}

/// 把一组链接折叠成轮播卡片：视频封面单独一张，
/// 视频按清晰度合并成一张，实况图与其动轨合并成一张。
fn slides_for_item(item: &ResultItem) -> Vec<SlideSpec> {
    if item.media_kind == MediaKind::Video {
        let quality_indices = video_entry_indices(item);
        if quality_indices.is_empty() {
            return Vec::new();
        }
        let mut slides = Vec::new();
        for cover_index in 0..item.covers.len() {
            slides.push(SlideSpec {
                kind: SlideKind::Cover,
                entry_index: 0,
                live_index: None,
                cover_index: Some(cover_index),
                quality_indices: Vec::new(),
            });
        }
        slides.push(SlideSpec {
            kind: SlideKind::Video,
            entry_index: quality_indices[0],
            live_index: None,
            cover_index: None,
            quality_indices,
        });
        return slides;
    }

    let mut slides = Vec::new();
    let mut index = 0;
    while index < item.entries.len() {
        let entry = &item.entries[index];
        if is_audio_entry(entry) {
            slides.push(SlideSpec {
                kind: SlideKind::Audio,
                entry_index: index,
                live_index: None,
                cover_index: None,
                quality_indices: vec![index],
            });
            index += 1;
            continue;
        }
        if is_live_entry(entry) {
            // 孤立的动轨（没有配对图片）按普通视频展示。
            slides.push(SlideSpec {
                kind: SlideKind::Video,
                entry_index: index,
                live_index: None,
                cover_index: None,
                quality_indices: vec![index],
            });
            index += 1;
            continue;
        }
        let live_index = item
            .entries
            .get(index + 1)
            .filter(|next| is_live_entry(next))
            .map(|_| index + 1);
        slides.push(SlideSpec {
            kind: if live_index.is_some() {
                SlideKind::Live
            } else {
                SlideKind::Image
            },
            entry_index: index,
            live_index,
            cover_index: None,
            quality_indices: vec![index],
        });
        index += if live_index.is_some() { 2 } else { 1 };
    }
    slides
}

/// 轮播相邻页的预览地址，仅供滑动动画使用（清晰度取默认档）。
fn slide_preview_url(item: &ResultItem, spec: &SlideSpec) -> Option<String> {
    let base_entry = if spec.kind == SlideKind::Cover {
        spec.cover_index.and_then(|index| item.covers.get(index))
    } else {
        item.entries.get(spec.entry_index)
    };
    let selected_entry = spec
        .quality_indices
        .first()
        .and_then(|index| item.entries.get(*index))
        .or(base_entry);
    match spec.kind {
        SlideKind::Audio => None,
        SlideKind::Cover => selected_entry.and_then(|entry| {
            entry
                .thumbnail_url
                .clone()
                .or_else(|| Some(entry.url.clone()))
        }),
        SlideKind::Image | SlideKind::Live => base_entry.and_then(|entry| {
            entry
                .thumbnail_url
                .clone()
                .or_else(|| entry.cover_url.clone())
                .or_else(|| Some(entry.url.clone()))
        }),
        SlideKind::Video => selected_entry.and_then(|entry| {
            entry
                .thumbnail_url
                .clone()
                .or_else(|| entry.cover_url.clone())
        }),
    }
}

fn duration_text(seconds: u64) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes >= 60 {
        format!("{}:{:02}:{:02}", minutes / 60, minutes % 60, seconds)
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn resolution_text(entry: &MediaEntry) -> Option<String> {
    match (entry.width, entry.height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => Some(format!("{width}×{height}")),
        _ => None,
    }
}

fn meta_right_text(entry: &MediaEntry, with_duration: bool) -> Option<String> {
    let resolution = resolution_text(entry);
    let duration = if with_duration {
        entry.duration_secs.map(duration_text)
    } else {
        None
    };
    match (resolution, duration) {
        (Some(resolution), Some(duration)) => Some(format!("{resolution} · {duration}")),
        (Some(resolution), None) => Some(resolution),
        (None, Some(duration)) => Some(duration),
        (None, None) => None,
    }
}

fn entry_ratio(entry: &MediaEntry) -> Option<f32> {
    match (entry.width, entry.height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => {
            Some(width as f32 / height as f32)
        }
        _ => None,
    }
}

fn fetch_thumbnail(url: &str, cache_dir: &std::path::Path) -> Result<PathBuf, ()> {
    use std::io::Read;

    std::fs::create_dir_all(cache_dir).map_err(|_| ())?;
    let key = douyin_core::api::md5_hex(url);
    let path = cache_dir.join(format!("{key}.img"));
    let jpeg_path = cache_dir.join(format!("{key}.jpg"));
    if jpeg_path.is_file() {
        return Ok(jpeg_path);
    }
    if !path.is_file() {
        // 与下载原图的请求保持一致的 UA / Referer，部分 CDN 只认下载用的这组头。
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|_| ())?;
        let response = client
            .get(url)
            .header(reqwest::header::USER_AGENT, crate::download::USER_AGENT)
            .header(reqwest::header::REFERER, crate::download::referer_for(url))
            .send()
            .map_err(|_| ())?;
        if !response.status().is_success() {
            return Err(());
        }
        let bytes = response.bytes().map_err(|_| ())?;
        if bytes.is_empty() {
            return Err(());
        }
        std::fs::write(&path, &bytes).map_err(|_| ())?;
    }
    let mut header = [0u8; 12];
    let mut file = std::fs::File::open(&path).map_err(|_| ())?;
    let read = file.read(&mut header).map_err(|_| ())?;
    if crate::image_convert::needs_platform_decoder(&header[..read]) {
        if crate::image_convert::convert_to_jpeg(&path, &jpeg_path, 2048, 90) {
            let _ = std::fs::remove_file(&path);
            return Ok(jpeg_path);
        }
        return Err(());
    }
    Ok(path)
}

impl AppView {
    pub fn toggle_result_view(&mut self, cx: &mut Context<Self>) {
        self.result_view = match self.result_view {
            ResultViewMode::Geek => ResultViewMode::Simple,
            ResultViewMode::Simple => ResultViewMode::Geek,
        };
        self.settings.result_view = self.result_view.setting_value().to_string();
        self.save_settings();
        self.experience_quality_menu = false;
        cx.notify();
    }

    pub fn experience_step_item(&mut self, delta: i32, cx: &mut Context<Self>) {
        let len = self.results.len();
        if len == 0 {
            return;
        }
        let next = (self.experience_item as i32 + delta).clamp(0, len as i32 - 1) as usize;
        if next != self.experience_item {
            self.experience_item = next;
            self.experience_slide = 0;
            self.experience_quality_menu = false;
            self.reset_experience_slide_gesture();
            cx.notify();
        }
    }

    pub fn experience_step_slide(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(item) = self.results.get(self.experience_item) else {
            return;
        };
        let count = slides_for_item(item).len();
        if count == 0 {
            return;
        }
        let next = (self.experience_slide as i32 + delta).clamp(0, count as i32 - 1) as usize;
        if next != self.experience_slide {
            self.experience_slide = next;
            self.experience_quality_menu = false;
            self.reset_experience_slide_gesture();
            cx.notify();
        }
    }

    /// 丢弃横滑状态并停止未完成的动画。
    pub fn reset_experience_slide_gesture(&mut self) {
        self.experience_slide_seq = self.experience_slide_seq.wrapping_add(1);
        self.experience_slide_drag = None;
        self.experience_slide_anim = None;
        self.experience_slide_offset = 0.0;
    }

    pub fn begin_experience_slide_drag(&mut self, x: f32, cx: &mut Context<Self>) {
        self.experience_slide_seq = self.experience_slide_seq.wrapping_add(1);
        self.experience_slide_anim = None;
        self.experience_slide_drag = Some(crate::SlideDrag { start_x: x });
        self.experience_slide_offset = 0.0;
        cx.notify();
    }

    pub fn update_experience_slide_drag(&mut self, x: f32, width: f32, cx: &mut Context<Self>) {
        let Some(start_x) = self.experience_slide_drag.as_ref().map(|drag| drag.start_x) else {
            return;
        };
        let Some(item) = self.results.get(self.experience_item) else {
            return;
        };
        let count = slides_for_item(item).len();
        let mut dx = x - start_x;
        // 到头后加阻尼，避免拖出空白。
        let at_start = self.experience_slide == 0;
        let at_end = self.experience_slide + 1 >= count;
        if (dx < 0.0 && at_end) || (dx > 0.0 && at_start) {
            dx *= 0.3;
        }
        let limit = width.max(1.0);
        self.experience_slide_offset = dx.clamp(-limit, limit);
        cx.notify();
    }

    pub fn finish_experience_slide_drag(&mut self, width: f32, cx: &mut Context<Self>) {
        if self.experience_slide_drag.take().is_none() {
            return;
        }
        let count = self
            .results
            .get(self.experience_item)
            .map(|item| slides_for_item(item).len())
            .unwrap_or(0);
        let width = width.max(1.0);
        let offset = self.experience_slide_offset;
        let threshold = width * 0.18;
        let delta = if offset <= -threshold && self.experience_slide + 1 < count {
            1
        } else if offset >= threshold && self.experience_slide > 0 {
            -1
        } else {
            0
        };
        let target = match delta {
            1 => -width,
            -1 => width,
            _ => 0.0,
        };
        self.start_experience_slide_animation(offset, target, delta, cx);
    }

    pub fn cancel_experience_slide_drag(&mut self, cx: &mut Context<Self>) {
        if self.experience_slide_drag.take().is_none() {
            return;
        }
        let from = self.experience_slide_offset;
        self.start_experience_slide_animation(from, 0.0, 0, cx);
    }

    fn start_experience_slide_animation(
        &mut self,
        from: f32,
        to: f32,
        delta: i32,
        cx: &mut Context<Self>,
    ) {
        if (from - to).abs() < 0.5 {
            self.experience_slide_anim = None;
            self.experience_slide_offset = 0.0;
            cx.notify();
            return;
        }
        self.experience_slide_seq = self.experience_slide_seq.wrapping_add(1);
        let seq = self.experience_slide_seq;
        self.experience_slide_anim = Some(crate::SlideAnim {
            started_at: Instant::now(),
            from,
            to,
            delta,
        });
        cx.spawn(async move |this: WeakEntity<AppView>, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let outcome = this.update(cx, |this, cx| {
                if this.experience_slide_seq != seq {
                    return true;
                }
                let running = this.advance_experience_slide();
                cx.notify();
                !running
            });
            match outcome {
                Ok(true) | Err(_) => break,
                Ok(false) => {}
            }
        })
        .detach();
    }

    /// 推进轮播位移动画；返回 true 表示还需要下一帧。
    fn advance_experience_slide(&mut self) -> bool {
        let Some(anim) = self.experience_slide_anim.as_ref() else {
            return false;
        };
        let elapsed = anim.started_at.elapsed().as_secs_f32();
        let linear = (elapsed / SLIDE_SNAP_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - linear).powi(3);
        let offset = anim.from + (anim.to - anim.from) * eased;
        let delta = anim.delta;
        if linear >= 1.0 {
            self.experience_slide_anim = None;
            self.experience_slide_offset = 0.0;
            if delta != 0 {
                let count = self
                    .results
                    .get(self.experience_item)
                    .map(|item| slides_for_item(item).len())
                    .unwrap_or(0);
                self.experience_slide = (self.experience_slide as i32 + delta)
                    .clamp(0, count.saturating_sub(1) as i32)
                    as usize;
                self.experience_quality_menu = false;
            }
            return false;
        }
        self.experience_slide_offset = offset;
        true
    }

    pub fn experience_select_quality(&mut self, entry_index: usize, cx: &mut Context<Self>) {
        self.experience_quality
            .insert(self.experience_item, entry_index);
        self.experience_quality_menu = false;
        cx.notify();
    }

    fn ensure_thumbnail(&mut self, url: &str, cx: &mut Context<Self>) {
        if self.thumbnails.contains_key(url) {
            return;
        }
        self.thumbnails.insert(url.to_string(), ThumbState::Loading);
        let key = url.to_string();
        let cache_dir = self.paths.cache_dir.join("thumbs");
        let fetch_key = key.clone();
        cx.spawn(async move |this: WeakEntity<AppView>, cx| {
            let fetched = cx
                .background_executor()
                .spawn(async move { fetch_thumbnail(&fetch_key, &cache_dir) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let state = match fetched {
                    Ok(path) => ThumbState::Ready(path),
                    Err(()) => ThumbState::Failed,
                };
                this.thumbnails.insert(key, state);
                cx.notify();
            });
        })
        .detach();
    }

    /// 清掉失败的预览状态与缓存文件，重新拉取。
    pub fn retry_experience_preview(&mut self, urls: &[String], cx: &mut Context<Self>) {
        let cache_dir = self.paths.cache_dir.join("thumbs");
        for url in urls {
            self.thumbnails.remove(url);
            let key = douyin_core::api::md5_hex(url);
            let _ = std::fs::remove_file(cache_dir.join(format!("{key}.img")));
        }
        cx.notify();
    }

    fn experience_quality_index(&self, quality_indices: &[usize]) -> usize {
        self.experience_quality
            .get(&self.experience_item)
            .copied()
            .and_then(|entry_index| {
                quality_indices
                    .iter()
                    .position(|index| *index == entry_index)
            })
            .unwrap_or(0)
    }

    fn experience_export(&mut self, title: &str, entries: Vec<MediaEntry>, cx: &mut Context<Self>) {
        #[cfg(target_os = "windows")]
        {
            self.pending_single_export = Some(crate::home::SingleExport {
                title: title.to_string(),
                entries,
            });
            self.dialog = Some(crate::Dialog::ExportMethod);
            cx.notify();
            return;
        }
        #[cfg(not(target_os = "windows"))]
        {
            let script_entries = entries
                .iter()
                .map(|entry| douyin_core::script::ScriptEntry {
                    title: title.to_string(),
                    url: entry.url.clone(),
                    extension: entry.extension.clone(),
                })
                .collect();
            self.confirm_export_entries(script_entries, cx);
        }
    }

    pub fn render_results_page_experience(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.results.is_empty() {
            return div()
                .flex_1()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(palette.text_muted)
                        .child("没有解析结果"),
                )
                .into_any_element();
        }
        if self.experience_item >= self.results.len() {
            self.experience_item = 0;
            self.experience_slide = 0;
            self.reset_experience_slide_gesture();
        }

        let tab_bar = self.render_experience_tab_bar(palette, cx);
        let body = match self.experience_tab {
            ExperienceTab::All => self.render_experience_all_tab(palette, window, cx),
            ExperienceTab::Title => self.render_experience_title_tab(palette, cx),
        };

        div()
            .flex_1()
            .w_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(tab_bar)
            .child(body)
            .into_any_element()
    }

    fn render_experience_tab_bar(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self.results.len();
        let can_prev = self.experience_item > 0;
        let can_next = self.experience_item + 1 < count;

        let entity = cx.entity();
        let mut tabs = div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(12.0));
        for (tab, label) in [(ExperienceTab::All, "全部"), (ExperienceTab::Title, "标题")] {
            let active = self.experience_tab == tab;
            let tab_entity = entity.clone();
            tabs = tabs.child(
                div()
                    .id(format!("experience-tab-{label}"))
                    .relative()
                    .h_full()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        tab_entity.update(cx, |this, cx| {
                            if this.experience_tab != tab {
                                this.experience_tab = tab;
                                this.experience_quality_menu = false;
                                cx.notify();
                            }
                        });
                    })
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(if active {
                                palette.primary
                            } else {
                                palette.text_secondary
                            })
                            .child(label),
                    )
                    .when(active, |el| {
                        el.child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .right(px(0.0))
                                .bottom(px(0.0))
                                .h(px(2.0))
                                .rounded_full()
                                .bg(palette.primary),
                        )
                    }),
            );
        }

        let entity = cx.entity();
        let prev = experience_nav_button(
            "experience-prev-item",
            "上一个",
            can_prev,
            palette,
            move |cx| {
                entity.update(cx, |this, cx| this.experience_step_item(-1, cx));
            },
        );
        let entity = cx.entity();
        let next = experience_nav_button(
            "experience-next-item",
            "下一个",
            can_next,
            palette,
            move |cx| {
                entity.update(cx, |this, cx| this.experience_step_item(1, cx));
            },
        );

        div()
            .w_full()
            .h(px(TAB_BAR_HEIGHT))
            .px(px(PAGE_PADDING))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(palette.divider)
            .child(prev)
            .child(tabs)
            .child(next)
            .into_any_element()
    }

    fn render_experience_all_tab(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (
            title,
            spec,
            base_entry,
            live_entry,
            quality_entries,
            quality_index,
            slide_index,
            slide_count,
            available_w,
            available_h,
            neighbor_url,
        ) = {
            let Some(item) = self.results.get(self.experience_item) else {
                return div().into_any_element();
            };
            let slides = slides_for_item(item);
            if slides.is_empty() {
                return div()
                    .w_full()
                    .py(px(48.0))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(palette.text_muted)
                            .child("暂无可预览的内容"),
                    )
                    .into_any_element();
            }
            let slide_count = slides.len();
            let slide_index = self.experience_slide.min(slide_count - 1);
            let spec = slides[slide_index].clone();
            let base_entry = if spec.kind == SlideKind::Cover {
                spec.cover_index
                    .and_then(|index| item.covers.get(index).cloned())
            } else {
                item.entries.get(spec.entry_index).cloned()
            };
            let Some(base_entry) = base_entry else {
                return div().into_any_element();
            };
            let live_entry = spec
                .live_index
                .and_then(|index| item.entries.get(index).cloned());
            let quality_entries: Vec<MediaEntry> = spec
                .quality_indices
                .iter()
                .filter_map(|index| item.entries.get(*index).cloned())
                .collect();
            let quality_index = self.experience_quality_index(&spec.quality_indices);
            let (available_w, available_h) = experience_preview_area(window);
            let slide_offset = self.experience_slide_offset;
            let neighbor_spec = if slide_offset < -1.0 {
                slides.get(slide_index + 1)
            } else if slide_offset > 1.0 {
                slide_index
                    .checked_sub(1)
                    .and_then(|index| slides.get(index))
            } else {
                None
            };
            let neighbor_url = neighbor_spec.and_then(|spec| slide_preview_url(item, spec));
            (
                item.title.clone(),
                spec,
                base_entry,
                live_entry,
                quality_entries,
                quality_index,
                slide_index,
                slide_count,
                available_w,
                available_h,
                neighbor_url,
            )
        };

        if let Some(url) = neighbor_url.as_deref() {
            self.ensure_thumbnail(url, cx);
        }
        let neighbor_path =
            neighbor_url
                .as_deref()
                .and_then(|url| match self.thumbnails.get(url) {
                    Some(ThumbState::Ready(path)) => Some(path.clone()),
                    _ => None,
                });

        let quality_index = quality_index.min(quality_entries.len().saturating_sub(1));
        let selected_entry = quality_entries
            .get(quality_index)
            .cloned()
            .unwrap_or_else(|| base_entry.clone());
        let is_video = spec.kind == SlideKind::Video;
        let is_live = spec.kind == SlideKind::Live;
        let is_audio = spec.kind == SlideKind::Audio;
        let is_cover = spec.kind == SlideKind::Cover;

        let primary_preview_url = if is_audio {
            None
        } else if is_cover {
            selected_entry
                .thumbnail_url
                .clone()
                .or_else(|| Some(selected_entry.url.clone()))
        } else if spec.kind == SlideKind::Image || is_live {
            base_entry
                .thumbnail_url
                .clone()
                .or_else(|| base_entry.cover_url.clone())
                .or_else(|| Some(base_entry.url.clone()))
        } else {
            selected_entry
                .thumbnail_url
                .clone()
                .or_else(|| selected_entry.cover_url.clone())
        };
        // 缩略图拉取失败时回退到原图下载链接，原图也失败才提示。
        let fallback_preview_url = if is_cover {
            Some(selected_entry.url.clone())
        } else if spec.kind == SlideKind::Image || is_live {
            Some(base_entry.url.clone())
        } else {
            None
        }
        .filter(|url| !url.is_empty() && Some(url) != primary_preview_url.as_ref());
        let preview_url = match primary_preview_url.as_ref() {
            Some(url) if matches!(self.thumbnails.get(url), Some(ThumbState::Failed)) => {
                fallback_preview_url.clone().or_else(|| Some(url.clone()))
            }
            other => other.cloned(),
        };
        let mut preview_retry_urls: Vec<String> = Vec::new();
        if let Some(url) = primary_preview_url.as_ref() {
            preview_retry_urls.push(url.clone());
        }
        if let Some(url) = fallback_preview_url.as_ref() {
            if !preview_retry_urls.contains(url) {
                preview_retry_urls.push(url.clone());
            }
        }
        if let Some(url) = preview_url.as_deref() {
            self.ensure_thumbnail(url, cx);
        }
        let preview_path = preview_url
            .as_deref()
            .and_then(|url| match self.thumbnails.get(url) {
                Some(ThumbState::Ready(path)) => Some(path.clone()),
                _ => None,
            });
        let preview_failed = preview_url
            .as_deref()
            .is_some_and(|url| matches!(self.thumbnails.get(url), Some(ThumbState::Failed)));
        let preview_hint = if preview_failed {
            "预览加载失败"
        } else {
            "预览加载中…"
        };

        let (card_w, card_h) = if is_audio {
            (available_w.min(520.0), available_h.min(320.0))
        } else {
            // 视频卡片按封面缩略图比例展示，与图片预览保持同样大小；
            // 直接按视频分辨率（如 9:16）会把封面图拉窄成一条。
            let ratio = if is_video {
                selected_entry
                    .thumbnail_width
                    .zip(selected_entry.thumbnail_height)
                    .filter(|(width, height)| *width > 0 && *height > 0)
                    .map(|(width, height)| width as f32 / height as f32)
                    .or_else(|| entry_ratio(&selected_entry))
            } else {
                entry_ratio(&selected_entry)
            };
            match ratio {
                Some(ratio) => {
                    let mut width = available_w;
                    let mut height = width / ratio;
                    if height > available_h {
                        height = available_h;
                        width = height * ratio;
                    }
                    (width.max(1.0), height.max(1.0))
                }
                None => (available_w, available_h),
            }
        };

        let mut card = div()
            .relative()
            .w(px(card_w))
            .h(px(card_h))
            .flex_shrink_0()
            .overflow_hidden()
            .bg(if is_audio {
                palette.path_bg
            } else {
                palette.result_item_bg
            });

        let preview: AnyElement = if is_audio {
            div()
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(15.0))
                        .text_color(palette.text_muted)
                        .child("音频文件"),
                )
                .into_any_element()
        } else if let Some(path) = preview_path {
            img(path)
                .w_full()
                .h_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        } else if preview_failed {
            let retry_bg = if palette.dark {
                rgba(0xffffff14)
            } else {
                rgba(0x00000010)
            };
            let retry_bg_hover = if palette.dark {
                rgba(0xffffff26)
            } else {
                rgba(0x0000001f)
            };
            let retry_entity = cx.entity();
            let retry_urls = preview_retry_urls.clone();
            div()
                .w_full()
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .child(
                    div()
                        .id("experience-preview-retry")
                        .w(px(40.0))
                        .h(px(40.0))
                        .rounded_full()
                        .bg(retry_bg)
                        .cursor_pointer()
                        .occlude()
                        .hover(move |style| style.bg(retry_bg_hover))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_click(move |_, _, cx| {
                            let urls = retry_urls.clone();
                            retry_entity.update(cx, |this, cx| {
                                this.retry_experience_preview(&urls, cx);
                            });
                        })
                        .child(widgets::icon(icons::RETRY, 22.0, palette.text_muted)),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(palette.text_muted)
                        .child(preview_hint),
                )
                .into_any_element()
        } else {
            div()
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(palette.text_muted)
                .child(preview_hint)
                .into_any_element()
        };

        let slide_offset = self.experience_slide_offset;
        let mut slides_layer = div()
            .id("experience-slide-layer")
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .w(px(card_w))
            .h(px(card_h));
        if !is_desktop() {
            let entity = cx.entity();
            let down = entity.clone();
            let moved = entity.clone();
            let up = entity.clone();
            slides_layer = slides_layer
                .on_pointer_down(move |event, window, cx| {
                    if window.capture_direct_pointer(event.pointer_id) {
                        let x = f32::from(event.position.x);
                        down.update(cx, |this, cx| {
                            this.begin_experience_slide_drag(x, cx);
                        });
                    }
                })
                .on_pointer_move(move |event, _, cx| {
                    let x = f32::from(event.position.x);
                    moved.update(cx, |this, cx| {
                        this.update_experience_slide_drag(x, card_w, cx);
                    });
                })
                .on_pointer_up(move |_, _, cx| {
                    up.update(cx, |this, cx| {
                        this.finish_experience_slide_drag(card_w, cx);
                    });
                })
                .on_pointer_cancel(move |_, _, cx| {
                    entity.update(cx, |this, cx| this.cancel_experience_slide_drag(cx));
                });
        }
        slides_layer = slides_layer.child(
            div()
                .absolute()
                .left(px(slide_offset))
                .top(px(0.0))
                .w(px(card_w))
                .h(px(card_h))
                .child(preview),
        );
        if let Some(path) = neighbor_path {
            let neighbor_left = if slide_offset < 0.0 {
                slide_offset + card_w
            } else {
                slide_offset - card_w
            };
            slides_layer = slides_layer.child(
                div()
                    .absolute()
                    .left(px(neighbor_left))
                    .top(px(0.0))
                    .w(px(card_w))
                    .h(px(card_h))
                    .child(img(path).w_full().h_full().object_fit(ObjectFit::Contain)),
            );
        }
        card = card.child(slides_layer);

        if is_live {
            card = card.child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .top(px(10.0))
                    .h(px(24.0))
                    .px(px(8.0))
                    .rounded_full()
                    .bg(rgba(0x00000080))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        gpui::svg()
                            .data(icons::LIVE_PHOTO_WHITE.as_bytes())
                            .text_color(rgb(0xffffff))
                            .w(px(16.0))
                            .h(px(16.0)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(14.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(0xffffff))
                            .child("LIVE"),
                    ),
            );
        }

        if is_video {
            card = card.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .w_full()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(54.0))
                            .h(px(54.0))
                            .rounded_full()
                            .bg(rgba(0x00000047))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(widgets::icon(icons::PLAY_OVERLAY, 30.0, rgb(0xffffff))),
                    ),
            );
        }

        if let Some(size) = selected_entry.size_bytes {
            card = card.child(overlay_pill(format_size(size), false));
        }
        if let Some(meta) = meta_right_text(&selected_entry, is_video) {
            card = card.child(overlay_pill(meta, true));
        }

        let quality_choice = is_video && quality_entries.len() > 1;
        if quality_choice {
            let label = selected_entry
                .quality
                .clone()
                .filter(|quality| !quality.trim().is_empty())
                .unwrap_or_else(|| format!("清晰度 {}", quality_index + 1));
            let toggle_entity = cx.entity();
            card = card.child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(8.0))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id("experience-quality-toggle")
                            .h(px(22.0))
                            .px(px(8.0))
                            .rounded_full()
                            .bg(rgba(0x00000073))
                            .cursor_pointer()
                            .occlude()
                            .hover(|style| style.bg(rgba(0x000000a6)))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .on_click(move |_, _, cx| {
                                toggle_entity.update(cx, |this, cx| {
                                    this.experience_quality_menu = !this.experience_quality_menu;
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .line_height(px(14.0))
                                    .text_color(rgb(0xffffff))
                                    .whitespace_nowrap()
                                    .child(label),
                            )
                            .child(widgets::icon(icons::CHEVRON_DOWN, 12.0, rgb(0xffffff))),
                    ),
            );

            if self.experience_quality_menu {
                let mut menu = div()
                    .w(px(128.0))
                    .p(px(4.0))
                    .rounded(px(6.0))
                    .bg(rgba(0x1f1f1fee))
                    .occlude()
                    .flex()
                    .flex_col();
                for (position, entry) in quality_entries.iter().enumerate() {
                    let entry_index = spec.quality_indices[position];
                    let active = position == quality_index;
                    let item_entity = cx.entity();
                    let label = entry
                        .quality
                        .clone()
                        .filter(|quality| !quality.trim().is_empty())
                        .unwrap_or_else(|| format!("清晰度 {}", position + 1));
                    menu = menu.child(
                        div()
                            .id(format!("experience-quality-{position}"))
                            .h(px(26.0))
                            .px(px(8.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|style| style.bg(rgba(0xffffff1f)))
                            .flex()
                            .items_center()
                            .justify_between()
                            .on_click(move |_, _, cx| {
                                item_entity.update(cx, |this, cx| {
                                    this.experience_select_quality(entry_index, cx);
                                });
                            })
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(if active {
                                        palette.primary
                                    } else {
                                        rgb(0xffffff)
                                    })
                                    .child(label),
                            )
                            .when(active, |el| {
                                el.child(widgets::icon(icons::CHECK, 12.0, palette.primary))
                            }),
                    );
                }
                card = card.child(
                    div()
                        .absolute()
                        .left(px(0.0))
                        .right(px(0.0))
                        .bottom(px(40.0))
                        .flex()
                        .justify_center()
                        .child(menu),
                );
            }
        }

        if is_cover {
            card = card.child(overlay_tag("封面"));
        }

        let can_prev = slide_index > 0;
        let can_next = slide_index + 1 < slide_count;
        // 两侧留白足够时把箭头放到预览图外侧（贴着图片留 8px），
        // 不足时仍叠加在图片内部。
        let side_blank = (available_w - card_w) / 2.0;
        let outside_inset =
            (side_blank >= ARROW_SLOT_WIDTH + 12.0).then(|| side_blank - ARROW_SLOT_WIDTH - 8.0);
        let mut outside_arrows: Vec<AnyElement> = Vec::new();
        if can_prev {
            let entity = cx.entity();
            let arrow = carousel_arrow(true, entity, palette, outside_inset);
            match outside_inset {
                Some(_) => outside_arrows.push(arrow),
                None => card = card.child(arrow),
            }
        }
        if can_next {
            let entity = cx.entity();
            let arrow = carousel_arrow(false, entity, palette, outside_inset);
            match outside_inset {
                Some(_) => outside_arrows.push(arrow),
                None => card = card.child(arrow),
            }
        }

        let copy_text = if is_live {
            match live_entry.as_ref() {
                Some(live) => format!("{}\n{}", base_entry.url, live.url),
                None => base_entry.url.clone(),
            }
        } else {
            selected_entry.url.clone()
        };
        let copy_message = if is_live {
            "已复制图片与视频链接"
        } else if is_cover {
            "已复制封面链接"
        } else {
            "已复制链接"
        };
        let copy_entity = cx.entity();
        let copy_button =
            ActionButton::new("experience-copy-button", "复制链接", palette.copy_button())
                .icon(icons::COPY, 16.0)
                .height(ACTION_ROW_HEIGHT)
                .on_click(move |_, _, cx| {
                    copy_entity.update(cx, |this, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                        this.toast(copy_message, ToastKind::Success, cx);
                    });
                });

        let (download_label, download_enabled) = match spec.kind {
            SlideKind::Video => ("下载视频", true),
            SlideKind::Audio => ("下载音频", is_desktop()),
            SlideKind::Image | SlideKind::Live | SlideKind::Cover => ("下载图片", true),
        };
        let download_entries = if is_live {
            let mut entries = vec![base_entry.clone()];
            if let Some(live) = live_entry.clone() {
                entries.push(live);
            }
            entries
        } else {
            vec![selected_entry.clone()]
        };
        let live_mode =
            is_live && crate::supports_live_photo_export() && self.settings.live_photo_export;
        let download_title = if is_cover {
            format!("{title}_封面")
        } else {
            title.clone()
        };
        let download_entity = cx.entity();
        let download_entries_for_click = download_entries.clone();
        let click_title = download_title.clone();
        let download_button = ActionButton::new(
            "experience-download-button",
            download_label,
            palette.download_button(),
        )
        .icon(icons::DOWNLOAD, 16.0)
        .height(ACTION_ROW_HEIGHT)
        .enabled(download_enabled)
        .on_click(move |_, _, cx| {
            let entries = download_entries_for_click.clone();
            let title = click_title.clone();
            download_entity.update(cx, |this, cx| {
                this.confirm_single_download(&title, entries, live_mode, cx);
            });
        });

        let mut action_row = div()
            .w_full()
            .px(px(PAGE_PADDING))
            .pt(px(ACTION_ROW_TOP))
            .pb(px(ACTION_ROW_BOTTOM))
            .flex()
            .gap(px(10.0))
            .child(div().flex_1().child(copy_button));

        if is_desktop() {
            let export_label = if cfg!(target_os = "windows") {
                "导出与下载"
            } else {
                "导出下载脚本"
            };
            let export_entries = download_entries.clone();
            let export_title = download_title.clone();
            let export_entity = cx.entity();
            action_row = action_row.child(
                div().flex_1().child(
                    ActionButton::new(
                        "experience-export-button",
                        export_label,
                        palette.export_button(),
                    )
                    .icon(icons::TERMINAL, 16.0)
                    .height(ACTION_ROW_HEIGHT)
                    .on_click(move |_, _, cx| {
                        let entries = export_entries.clone();
                        let title = export_title.clone();
                        export_entity.update(cx, |this, cx| {
                            this.experience_export(&title, entries, cx);
                        });
                    }),
                ),
            );
        }

        action_row = action_row.child(div().flex_1().child(download_button));

        let mut preview_area = div()
            .relative()
            .flex_1()
            .w_full()
            .min_h(px(0.0))
            .px(px(PAGE_PADDING))
            .py(px(PREVIEW_PADDING_V))
            .flex()
            .items_center()
            .justify_center()
            .child(card);
        for arrow in outside_arrows {
            preview_area = preview_area.child(arrow);
        }

        div()
            .flex_1()
            .w_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(preview_area)
            .child(action_row)
            .into_any_element()
    }

    fn render_experience_title_tab(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self
            .results
            .get(self.experience_item)
            .map(|item| item.title.clone())
            .unwrap_or_default();
        let entity = cx.entity();
        let copy_title = title.clone();
        let box_bg = if self.dark() {
            palette.bg
        } else {
            palette.result_item_bg
        };

        div()
            .flex_1()
            .w_full()
            .min_h(px(0.0))
            .px(px(PAGE_PADDING))
            .pt(px(PREVIEW_PADDING_V))
            .pb(px(ACTION_ROW_BOTTOM))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("experience-title-scroll")
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .p(px(16.0))
                            .rounded(px(10.0))
                            .bg(box_bg)
                            .border_1()
                            .border_color(palette.divider)
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .line_height(px(26.0))
                                    .text_color(palette.text)
                                    .whitespace_normal()
                                    .child(title),
                            ),
                    ),
            )
            .child(
                div().mt(px(ACTION_ROW_TOP)).w_full().child(
                    ActionButton::new("experience-title-copy", "复制", palette.copy_button())
                        .icon(icons::COPY, 16.0)
                        .height(ACTION_ROW_HEIGHT)
                        .enabled(!copy_title.is_empty())
                        .on_click(move |_, _, cx| {
                            entity.update(cx, |this, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    copy_title.clone(),
                                ));
                                this.toast("已复制标题", ToastKind::Success, cx);
                            });
                        }),
                ),
            )
            .into_any_element()
    }
}

/// 体验版媒体区可用尺寸：窗口减去标题栏、标签栏、底部按钮与内边距。
fn experience_preview_area(window: &Window) -> (f32, f32) {
    let viewport = window.viewport_size();
    let mut chrome = if is_desktop() {
        crate::SYSTEM_BAR_HEIGHT + crate::TITLE_ROW_HEIGHT
    } else {
        crate::MOBILE_APPBAR_HEIGHT
    };
    let (safe_h, safe_v) = safe_area_size();
    chrome += safe_v;
    let available_w = (f32::from(viewport.width) - PAGE_PADDING * 2.0 - safe_h).max(120.0);
    let available_h = (f32::from(viewport.height)
        - chrome
        - TAB_BAR_HEIGHT
        - ACTION_ROW_HEIGHT
        - ACTION_ROW_TOP
        - ACTION_ROW_BOTTOM
        - PREVIEW_PADDING_V * 2.0
        - 8.0)
        .max(100.0);
    (available_w, available_h)
}

fn safe_area_size() -> (f32, f32) {
    #[cfg(target_os = "ios")]
    {
        let (top, left, bottom, right) = gpui_ios::safe_area_insets();
        (left + right, top + bottom)
    }
    #[cfg(target_os = "android")]
    {
        let (left, top, right, bottom) = gpui_android::safe_area_insets_logical();
        (left + right, top + bottom)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        (0.0, 0.0)
    }
}

fn experience_nav_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    palette: Palette,
    on_click: impl Fn(&mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .w(px(NAV_BUTTON_WIDTH))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(palette.primary)
        .whitespace_nowrap()
        .when(enabled, |el| {
            el.cursor_pointer()
                .hover(|style| style.opacity(0.6))
                .on_click(move |_, _, cx| on_click(cx))
        })
        .when(!enabled, |el| el.opacity(0.0))
        .child(label)
        .into_any_element()
}

fn overlay_pill(text: String, right: bool) -> AnyElement {
    let mut pill = div()
        .absolute()
        .bottom(px(8.0))
        .h(px(22.0))
        .px(px(8.0))
        .rounded_full()
        .bg(rgba(0x00000073))
        .flex()
        .items_center()
        .child(
            div()
                .text_size(px(11.0))
                .line_height(px(14.0))
                .text_color(rgb(0xffffff))
                .whitespace_nowrap()
                .child(text),
        );
    pill = if right {
        pill.right(px(8.0))
    } else {
        pill.left(px(8.0))
    };
    pill.into_any_element()
}

/// “封面”标签：与体积/分辨率同一水平线居中，样式与体积标签一致。
fn overlay_tag(text: &'static str) -> AnyElement {
    div()
        .absolute()
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(8.0))
        .flex()
        .justify_center()
        .child(
            div()
                .h(px(22.0))
                .px(px(8.0))
                .rounded_full()
                .bg(rgba(0x00000073))
                .flex()
                .items_center()
                .child(
                    div()
                        .text_size(px(11.0))
                        .line_height(px(14.0))
                        .text_color(rgb(0xffffff))
                        .whitespace_nowrap()
                        .child(text),
                ),
        )
        .into_any_element()
}

fn carousel_arrow(
    left: bool,
    entity: gpui::Entity<AppView>,
    palette: Palette,
    outside_inset: Option<f32>,
) -> AnyElement {
    // 图片内部的箭头固定为深色半透明底 + 白色图标；
    // 放到留白处时深色主题反转为浅色半透明底 + 深色图标。
    let (background, background_hover, icon_color) = if outside_inset.is_some() && palette.dark {
        (rgba(0xffffff33), rgba(0xffffff59), palette.bg)
    } else {
        (rgba(0x00000040), rgba(0x00000080), rgb(0xffffff))
    };
    let inset = px(outside_inset.unwrap_or(0.0));
    let mut arrow = div()
        .absolute()
        .top(px(0.0))
        .bottom(px(0.0))
        .w(px(ARROW_SLOT_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    arrow = if left {
        arrow.left(inset)
    } else {
        arrow.right(inset)
    };
    arrow
        .child(
            div()
                .id(if left {
                    "experience-slide-prev"
                } else {
                    "experience-slide-next"
                })
                .w(px(48.0))
                .h(px(48.0))
                .rounded_full()
                .bg(background)
                .cursor_pointer()
                .occlude()
                .hover(move |style| style.bg(background_hover))
                .flex()
                .items_center()
                .justify_center()
                .on_click(move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.experience_step_slide(if left { -1 } else { 1 }, cx);
                    });
                })
                .child(widgets::icon(
                    if left {
                        icons::CHEVRON_LEFT
                    } else {
                        icons::CHEVRON_RIGHT
                    },
                    28.0,
                    icon_color,
                )),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use douyin_core::extractor::{MediaEntry, MediaKind, MediaPlatform};

    use super::{slides_for_item, SlideKind};
    use crate::ResultItem;

    fn cover(url: &str) -> MediaEntry {
        MediaEntry {
            url: url.to_string(),
            extension: Some("jpg".to_string()),
            thumbnail_url: Some(url.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn video_slides_are_both_covers_then_video() {
        let item = ResultItem {
            title: "test".to_string(),
            platform: MediaPlatform::Douyin,
            media_kind: MediaKind::Video,
            entries: vec![MediaEntry {
                url: "https://example.com/video.mp4".to_string(),
                extension: Some("mp4".to_string()),
                thumbnail_url: Some("https://example.com/pc.jpg".to_string()),
                ..Default::default()
            }],
            covers: vec![
                cover("https://example.com/mobile.webp"),
                cover("https://example.com/pc.jpg"),
            ],
        };
        let slides = slides_for_item(&item);
        assert_eq!(slides.len(), 3);
        assert!(matches!(slides[0].kind, SlideKind::Cover));
        assert_eq!(slides[0].cover_index, Some(0));
        assert!(matches!(slides[1].kind, SlideKind::Cover));
        assert_eq!(slides[1].cover_index, Some(1));
        assert!(matches!(slides[2].kind, SlideKind::Video));
        assert_eq!(slides[2].entry_index, 0);
    }
}
