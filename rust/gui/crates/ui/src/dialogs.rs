//! 对话框：帮助、关于、主题、存储、缓存、下载确认 / 进度 / 完成。

#[cfg(target_os = "android")]
use douyin_input::native_editor_owned_by;
use douyin_input::{deferred_input_state_reset, reset_input_state, Input, InputSize};
use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, rgb, AnyElement, App, ClickEvent, Context, Div, FontWeight, InteractiveElement,
    IntoElement, MouseButton, ParentElement, StatefulInteractiveElement, Styled, WeakEntity,
    Window,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use gpui::{Modifiers, MouseUpEvent, PlatformInput};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::paths::{format_size, is_desktop, open_directory};
use crate::theme::{ButtonColors, Palette};
use crate::widgets::{self, ProgressBar};
use crate::{icons, AppView, Dialog, ResourceScope, ToastKind, APP_NAME, APP_VERSION};

const SUPPORTED_FORMATS: [&str; 7] = [
    "5.12 复制打开抖音，看看【赵今麦angel的作品】小林～拍照📷# 剧剧抖有戏  https://v.douyin.com/HUbJB4-GQdc/ 03/26 e@b.nD yGi:/ ",
    "https://v.douyin.com/L8tSCYe-sC4",
    "https://www.douyin.com/video/7588361322312650038",
    "https://m.douyin.com/share/video/7588361322312650038",
    "https://www.iesdouyin.com/share/video/7588361322312650038",
    "https://www.douyin.com/jingxuan?modal_id=7701193143558901018",
    "https://www.douyin.com/jingxuan?modal_id=7674934407219793171",
];
#[cfg(any(target_os = "windows", target_os = "macos"))]
const IDM_HOMEPAGE_URL: &str = "https://www.internetdownloadmanager.com/";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const ARIA2C_URL: &str = "https://aria2.org/";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const ROBUST_DOWNLOADER_URL: &str = "https://github.com/nilaoda/RobustDownloader/releases";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const DOWNLOADER_URL: &str = "https://github.com/nilaoda/SharpTools/tree/main/downloader";
const SUPPORTED_XHS_FORMATS: [&str; 5] = [
    "75 【新巡演，整起！^_^ - 张杰 | 小红书 - 你的生活兴趣社区】 😆 RC8p7DRyJZO8KBy 😆 https://www.xiaohongshu.com/discovery/item/6a9e414d000000002a02d317?xsec_token=ABxxxx",
    "https://www.xiaohongshu.com/discovery/item/6a9e414d000000002a02d317?xsec_token=ABxxxx",
    "bloc1✖️愤怒攀岩👊😡👊 今天怒气值max，状态和能力... https://xhslink.cn/o/8C9r0KXJsjj 把口令复制下来，进入【小红书】看笔记~",
    "https://www.xiaohongshu.com/explore/6644e3a8000000001e01cc4d?xsec_token=ABxxxx",
    "https://www.xiaohongshu.com/user/profile/65c5fd5600000000110173a5/6773508e0000000009015fa0?xsec_token=ABxxxx",
];
pub(crate) const XHS_COOKIE_INPUT_ID: &str = "xhs-cookie-input";
static XHS_COOKIE_DIALOG_SEQ: AtomicU32 = AtomicU32::new(0);

fn next_xhs_cookie_dialog_seq() -> u32 {
    XHS_COOKIE_DIALOG_SEQ.fetch_add(1, Ordering::Relaxed) + 1
}

fn current_xhs_cookie_dialog_seq() -> u32 {
    XHS_COOKIE_DIALOG_SEQ.load(Ordering::Relaxed)
}

fn xhs_cookie_input_id() -> gpui::ElementId {
    gpui::ElementId::from(XHS_COOKIE_INPUT_ID)
}

fn cookie_input_bg(dark: bool) -> gpui::Rgba {
    if dark {
        gpui::rgb(0x2b2b2b)
    } else {
        gpui::rgb(0xffffff)
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn link_text(
    id: &'static str,
    label: &'static str,
    url: &'static str,
    palette: Palette,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .text_color(palette.primary)
        .underline()
        .text_decoration_color(palette.primary)
        .text_decoration_solid()
        .cursor_pointer()
        .child(label.to_string())
        .on_click(move |_, _, cx| cx.open_url(url))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn plain_text(label: &'static str) -> Div {
    div().child(label.to_string())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn dialog_text_line(color: gpui::Rgba) -> Div {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(0.0))
        .text_size(px(14.0))
        .line_height(px(22.0))
        .text_color(color)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn current_xhs_cookie_editor_text() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        if !native_editor_owned_by(&xhs_cookie_input_id()) {
            return None;
        }
        return gpui_android::native_editor_text();
    }
    #[cfg(target_os = "ios")]
    {
        return crate::ios_native_input::text_for(XHS_COOKIE_INPUT_ID);
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn sync_xhs_cookie_editor_text(view: &mut AppView) {
    if let Some(text) = current_xhs_cookie_editor_text() {
        view.xhs_cookie_input = text;
    }
}

fn reset_xhs_cookie_draft(saved_cookie: &str) {
    #[cfg(target_os = "android")]
    let owned_native_editor = native_editor_owned_by(&xhs_cookie_input_id());
    reset_input_state(&xhs_cookie_input_id(), saved_cookie);
    // Rust owns the Cookie field's fresh state. Only replace the shared
    // EditText when it actually belongs to this dialog; otherwise a Cookie
    // close would discard Home's live text, session, focus and height.
    #[cfg(target_os = "android")]
    if owned_native_editor {
        gpui_android::reset_native_editor_text(saved_cookie);
    }
    #[cfg(target_os = "ios")]
    {
        crate::ios_native_input::hide_placeholder(XHS_COOKIE_INPUT_ID);
        crate::ios_native_input::clear_height_events(XHS_COOKIE_INPUT_ID);
        crate::ios_native_input::set_text(XHS_COOKIE_INPUT_ID, saved_cookie);
    }
}

/// Close whichever dialog is open. The Cookie dialog always restores its
/// saved value, including when another app flow replaces it with a new one.
pub fn close_or_replace_dialog(view: &mut AppView, window: &mut Window, cx: &mut Context<AppView>) {
    let was_cookie = view
        .dialog
        .as_ref()
        .is_some_and(|dialog| matches!(dialog, Dialog::XhsCookie));
    if was_cookie {
        #[cfg(target_os = "ios")]
        {
            crate::ios_native_input::hide_placeholder(XHS_COOKIE_INPUT_ID);
            view.home_native_input_lines = None;
            view.xhs_cookie_native_input_lines = None;
        }
        let sequence = next_xhs_cookie_dialog_seq();
        let saved_cookie = view.settings.xhs_cookie.clone();
        let deferred_reset = deferred_input_state_reset(&xhs_cookie_input_id(), &saved_cookie);
        #[cfg(target_os = "android")]
        let owned_native_editor = native_editor_owned_by(&xhs_cookie_input_id());
        #[cfg(target_os = "ios")]
        let owned_native_editor = crate::ios_native_input::is_editing(XHS_COOKIE_INPUT_ID);
        #[cfg(target_os = "android")]
        let android_owned_native_editor = owned_native_editor;
        let cookie_had_focus = view.xhs_cookie_focus.is_focused(window);
        view.dialog = None;
        view.xhs_cookie_card_top = None;
        #[cfg(target_os = "android")]
        {
            view.xhs_cookie_warmup_frames = 0;
            view.xhs_cookie_editor_warmup = false;
            view.xhs_cookie_ime_follow = false;
            view.xhs_cookie_ime_preview = false;
            gpui_android::set_native_editor_warmup(false);
            gpui_android::set_native_editor_ime_follow(false);
            gpui_android::set_native_editor_ime_preview(false);
        }
        view.xhs_cookie_input = saved_cookie.clone();
        reset_xhs_cookie_draft(&saved_cookie);
        view.xhs_cookie_focus = cx.focus_handle();
        view.xhs_cookie_scroll = gpui::ScrollHandle::new();
        view.xhs_cookie_scrollbar_drag = widgets::ScrollbarDrag::default();
        #[cfg(target_os = "ios")]
        crate::ios_native_input::set_text(XHS_COOKIE_INPUT_ID, &saved_cookie);
        // The dialog is going away. Any Java mirror callbacks that were
        // already queued must not be treated as Home text, and Home's next
        // native-editor refresh must use Rust's current session.
        #[cfg(target_os = "android")]
        if android_owned_native_editor {
            gpui_android::discard_native_mirror_events();
            gpui_android::synchronize_native_editor_session();
            crate::dismiss_native_editor_now();
            crate::hide_soft_keyboard();
        }
        #[cfg(target_os = "android")]
        if cookie_had_focus || android_owned_native_editor {
            window.blur(cx);
        }
        #[cfg(target_os = "ios")]
        if cookie_had_focus || owned_native_editor {
            window.blur(cx);
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if cookie_had_focus {
            window.blur(cx);
        }
        cx.notify();
        cx.defer(move |cx| {
            if current_xhs_cookie_dialog_seq() != sequence {
                return;
            }
            let Some(handle) = cx.active_window() else {
                return;
            };
            let Some(handle) = handle.downcast::<AppView>() else {
                return;
            };
            let _ = handle.update(cx, |view, _, cx| {
                if current_xhs_cookie_dialog_seq() != sequence {
                    return;
                }
                deferred_reset();
                view.xhs_cookie_input = saved_cookie.clone();
                reset_xhs_cookie_draft(&saved_cookie);
                cx.notify();
            });
        });
        return;
    }
    view.dialog = None;
    cx.notify();
}

impl AppView {
    /// Window-space top of the Cookie dialog for the current viewport.
    ///
    /// Uses the same viewport/keyboard math as the overlay itself, so the
    /// native editor can be moved by the exact layout delta on every device.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub(crate) fn cookie_dialog_target_top(&self, window: &Window) -> Option<f32> {
        if !matches!(self.dialog, Some(Dialog::XhsCookie)) {
            return None;
        }
        let viewport = window.viewport_size();
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let landscape_cookie = viewport_width > viewport_height;
        let keyboard_height = {
            #[cfg(target_os = "android")]
            {
                gpui_android::ime_bottom_inset_logical()
            }
            #[cfg(target_os = "ios")]
            {
                gpui_ios::keyboard_height()
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                0.0
            }
        };
        let available_height = if landscape_cookie {
            viewport_height.max(1.0)
        } else {
            (viewport_height - keyboard_height).max(1.0)
        };
        Some(((available_height - self.xhs_cookie_card_height) * 0.5).max(0.0))
    }

    /// Move the native Cookie editor by the exact dialog-layout delta before
    /// GPUI paints the frame, so both surfaces target the same vsync.
    #[cfg(target_os = "android")]
    pub(crate) fn sync_cookie_dialog_native_offset(&mut self, window: &Window) {
        let target = self.cookie_dialog_target_top(window);
        let viewport = window.viewport_size();
        let portrait = f32::from(viewport.width) <= f32::from(viewport.height);
        let editor_owned = native_editor_owned_by(&xhs_cookie_input_id());
        // The card is centred by flex layout from the first frame; its height
        // is still unknown until the first prepaint. Do not treat the
        // placeholder height (zero) as a real position: moving the native
        // editor against that value is what made the first open jump upward.
        let height_known = self.xhs_cookie_card_height > 0.5;
        // In portrait the card reflows around the keyboard, so Android can
        // move the editor from its IME inset callback, one step earlier than
        // this render pass. Landscape centres the card without the keyboard;
        // it must not follow the IME. Keep following across the focus/blur
        // that hides the keyboard; the Java side no-ops when no editor is
        // attached and resets its inset baseline when one appears.
        let follow_ime = target.is_some() && portrait;
        if follow_ime != self.xhs_cookie_ime_follow {
            if follow_ime {
                if let (Some(previous), Some(target)) = (self.xhs_cookie_card_top, target) {
                    // Java's inset baseline starts at the current keyboard
                    // height. Align any layout change that happened before
                    // the hand-off so the next inset delta starts from the
                    // card.
                    let delta = target - previous;
                    if delta.abs() > 0.5 && editor_owned {
                        gpui_android::translate_native_editor(delta);
                    }
                }
            }
            gpui_android::set_native_editor_ime_follow(follow_ime);
            self.xhs_cookie_ime_follow = follow_ime;
        }
        if follow_ime != self.xhs_cookie_ime_preview {
            gpui_android::set_native_editor_ime_preview(follow_ime);
            self.xhs_cookie_ime_preview = follow_ime;
        }
        if height_known {
            if let (Some(previous), Some(target)) = (self.xhs_cookie_card_top, target) {
                let delta = target - previous;
                if delta.abs() > 0.5 && editor_owned && !follow_ime {
                    gpui_android::translate_native_editor(delta);
                }
            }
            self.xhs_cookie_card_top = target;
        }
    }

    pub fn render_dialog(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.dialog.clone()?;
        let is_cookie_dialog = matches!(dialog, Dialog::XhsCookie);
        #[cfg(target_os = "android")]
        if is_cookie_dialog && self.xhs_cookie_editor_warmup && self.xhs_cookie_warmup_frames == 0 {
            // This runs before the frame is drawn. The editor window has
            // already completed its attach pass at alpha 0, so revealing it
            // here puts it in the same compositor frame as the card.
            self.xhs_cookie_editor_warmup = false;
            gpui_android::set_native_editor_warmup(false);
        }
        let viewport = window.viewport_size();
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let keyboard_height = {
            #[cfg(target_os = "android")]
            {
                gpui_android::ime_bottom_inset_logical()
            }
            #[cfg(target_os = "ios")]
            {
                gpui_ios::keyboard_height()
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                0.0
            }
        };
        // A phone keyboard in landscape is normally a compact/floating
        // surface that overlays the page. Reflowing the Cookie card as if it
        // were portrait leaves a very short card and makes the native editor
        // escape the card bounds. Keep that dialog at its normal size in
        // landscape and centre it in the full viewport instead.
        let landscape_cookie = is_cookie_dialog
            && cfg!(any(target_os = "android", target_os = "ios"))
            && viewport_width > viewport_height;
        let available_height = if landscape_cookie {
            viewport_height.max(1.0)
        } else {
            (viewport_height - keyboard_height).max(1.0)
        };
        let card_width = (viewport_width - 48.0)
            .min(viewport_width * 0.8)
            .min(480.0)
            .max(1.0);
        let max_card_height = (available_height * 0.8).max(1.0);
        let card = match dialog {
            Dialog::Help => self.dialog_help(palette, card_width, max_card_height, cx),
            Dialog::About => self.dialog_about(palette, card_width, max_card_height, cx),
            Dialog::SaveLocation => {
                self.dialog_save_location(palette, card_width, max_card_height, cx)
            }
            Dialog::XhsCookie => self.dialog_xhs_cookie(
                palette,
                card_width,
                max_card_height,
                landscape_cookie,
                window,
                cx,
            ),
            Dialog::LivePhotoExport => {
                self.dialog_live_photo_export(palette, card_width, max_card_height, cx)
            }
            Dialog::ClearCache => self.dialog_clear_cache(palette, card_width, max_card_height, cx),
            Dialog::NoCache => self.dialog_no_cache(palette, card_width, max_card_height, cx),
            Dialog::ClearCacheResult {
                success,
                message,
                details,
            } => self.dialog_clear_result(
                palette,
                card_width,
                max_card_height,
                success,
                message,
                details,
                cx,
            ),
            Dialog::DownloadSelection => {
                self.dialog_download_selection(palette, card_width, max_card_height, cx)
            }
            #[cfg(target_os = "windows")]
            Dialog::ExportMethod => {
                self.dialog_export_method(palette, card_width, max_card_height, cx)
            }
            Dialog::ExportSelection => {
                self.dialog_export_selection(palette, card_width, max_card_height, cx)
            }
            #[cfg(target_os = "windows")]
            Dialog::IdmSelection => {
                self.dialog_idm_selection(palette, card_width, max_card_height, cx)
            }
            #[cfg(target_os = "windows")]
            Dialog::IdmResult { success, message } => {
                self.dialog_idm_result(palette, card_width, max_card_height, success, message, cx)
            }
            Dialog::DownloadProgress => {
                self.dialog_download_progress(palette, card_width, max_card_height, cx)
            }
            Dialog::DownloadSaved {
                count,
                failed,
                skipped,
            } => self.dialog_download_saved(
                palette,
                card_width,
                max_card_height,
                count,
                failed,
                skipped,
                cx,
            ),
            Dialog::DownloadSummary { files } => {
                self.dialog_download_summary(palette, card_width, max_card_height, files, cx)
            }
        };

        let mut card = card;
        if landscape_cookie {
            // In landscape the dialog keeps its normal, keyboard-independent
            // size. Use the full allowed height so the one-third editor cap
            // has a stable reference and cannot feed back into the card size.
            card = card.h(px(max_card_height));
        }

        let card_entity = cx.entity();
        let card = div()
            .on_children_prepainted(move |bounds, _, cx| {
                if !is_cookie_dialog {
                    return;
                }
                let Some(bounds) = bounds.first() else {
                    return;
                };
                let height = f32::from(bounds.size.height);
                card_entity.update(cx, |view, cx| {
                    let mut changed = false;
                    if (view.xhs_cookie_card_height - height).abs() > 0.5 {
                        view.xhs_cookie_card_height = height;
                        changed = true;
                    }
                    #[cfg(target_os = "android")]
                    if view.xhs_cookie_warmup_frames > 0 {
                        view.xhs_cookie_warmup_frames -= 1;
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                });
            })
            .child(card);

        let overlay = div()
            .absolute()
            .inset_0()
            .flex()
            .justify_center()
            .bg(gpui::rgba(0x00000066))
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                // 点击遮罩视为放弃输入。先只结束编辑并保留原生控件，随后
                // 的关闭流程会统一隐藏并重置 Cookie 字段；直接 hide-all
                // 会在重开弹窗时抢先执行异步隐藏，导致原生 placeholder
                // 被隐藏。
                crate::hide_soft_keyboard();
                window.blur(cx);
                let Some(handle) = cx.active_window() else {
                    return;
                };
                let Some(handle) = handle.downcast::<AppView>() else {
                    return;
                };
                let _ = handle.update(cx, |view, window, cx| {
                    if matches!(view.dialog, Some(Dialog::XhsCookie)) {
                        close_or_replace_dialog(view, window, cx);
                    }
                });
            })
            .child(
                div()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(card),
            );
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let overlay = if is_cookie_dialog {
            // Centre the card from the first frame. The previous top-offset
            // formula needed the measured card height, so the first frame
            // used zero and started below the final position, then jumped up.
            // Bottom padding keeps the keyboard-aware centre without knowing
            // the card height.
            #[cfg(target_os = "android")]
            let card_ready = self.xhs_cookie_warmup_frames == 0;
            #[cfg(target_os = "ios")]
            let card_ready = true;
            overlay
                .items_center()
                .pb(px(if landscape_cookie {
                    0.0
                } else {
                    keyboard_height
                }))
                .opacity(if card_ready { 1.0 } else { 0.0 })
        } else {
            overlay.items_center()
        };
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let overlay = overlay.items_center();

        Some(overlay.into_any_element())
    }

    fn dialog_card(&self, palette: Palette, width: f32, max_height: f32) -> Div {
        div()
            .w(px(width))
            .max_h(px(max_height))
            .min_h(px(0.0))
            .rounded(px(24.0))
            .bg(if palette.dark {
                gpui::rgb(0x2b2b2b)
            } else {
                gpui::rgb(0xffffff)
            })
            .p(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .overflow_hidden()
            .text_color(palette.text)
    }

    fn dialog_content_scroll(&self, id: &'static str) -> gpui::Stateful<Div> {
        div()
            .id(id)
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .scrollbar_width(px(8.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
    }

    fn dialog_title(&self, palette: Palette, text: &str, size: f32) -> Div {
        div()
            .text_size(px(size))
            .font_weight(FontWeight::MEDIUM)
            .text_color(palette.text)
            .child(text.to_string())
    }

    fn action_row(&self) -> Div {
        div()
            .w_full()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(px(12.0))
            .mt(px(8.0))
    }

    fn text_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        primary: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let colors = if primary {
            palette.download_button()
        } else {
            palette.plain_button()
        };
        self.dialog_action(id, label, colors, handler)
    }

    fn primary_action(
        &self,
        _palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.dialog_action(id, label, _palette.download_button(), handler)
    }

    fn outline_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.dialog_action(id, label, palette.outline_button(), handler)
    }

    fn storage_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.dialog_action(id, label, palette.outline_button(), handler)
    }

    fn storage_reset_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.dialog_action(id, label, palette.plain_button(), handler)
    }

    fn plain_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.dialog_action(id, label, palette.plain_button(), handler)
    }

    fn dialog_action(
        &self,
        id: &'static str,
        label: &str,
        colors: ButtonColors,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let text_width: f32 = label
            .chars()
            .map(|character| if character.is_ascii() { 8.0 } else { 14.0 })
            .sum();
        div()
            .w(px((text_width + 40.0).max(72.0)))
            .child(
                widgets::ActionButton::new(id, label, colors)
                    .height(34.0)
                    .corner_radius(8.0)
                    .on_click(handler),
            )
            .into_any_element()
    }

    /// Cookie actions use a stable GPUI click target. A focused mobile native
    /// editor can consume the press while resigning first responder, but the
    /// click must still close or save the dialog in one gesture.
    fn cookie_action(
        &self,
        id: &'static str,
        label: &str,
        colors: ButtonColors,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let text_width: f32 = label
            .chars()
            .map(|character| if character.is_ascii() { 8.0 } else { 14.0 })
            .sum();
        let handler = Rc::new(handler);
        let activated = Rc::new(Cell::new(false));
        let click_handler = handler.clone();
        let click_activated = activated.clone();
        let button = widgets::ActionButton::new(id, label, colors)
            .height(34.0)
            .corner_radius(8.0)
            .on_click(move |_, window, cx| {
                if !click_activated.replace(true) {
                    click_handler(window, cx);
                }
            });
        let target = div().w(px((text_width + 40.0).max(72.0))).child(button);
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let target = target.on_mouse_down(MouseButton::Left, move |_, window, cx| {
            // A focused native editor can lose first-responder state during
            // the press. Finish mobile dialog actions on press so the same
            // gesture cannot degrade into keyboard dismissal. The shared flag
            // prevents the following click from running the action twice.
            if !activated.replace(true) {
                handler(window, cx);
            }
            cx.stop_propagation();
        });
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let target = {
            let desktop_handler = handler.clone();
            let desktop_activated = activated.clone();
            target.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                // Desktop focus loss can consume the click that follows the
                // press. Activate on press so the first gesture always works;
                // the shared flag prevents the click from running it twice.
                if !desktop_activated.replace(true) {
                    desktop_handler(window, cx);
                }
            })
        };
        target.into_any_element()
    }

    fn dialog_help(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity();
        let format_rows = |items: &[&str]| -> Vec<gpui::AnyElement> {
            items
                .iter()
                .map(|format| {
                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .child(div().mt(px(2.0)).child(widgets::icon(
                            icons::CHECK_CIRCLE,
                            16.0,
                            gpui::rgb(0x05c001),
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .font_family(crate::theme::mono_font())
                                .text_size(px(13.0))
                                .text_color(palette.primary)
                                .child((*format).to_string()),
                        )
                        .into_any_element()
                })
                .collect()
        };

        self.dialog_card(palette, width, max_height)
            .child(
                self.dialog_title(palette, "使用说明", 18.0)
                    .flex_shrink_0(),
            )
            .child(
                self.dialog_content_scroll("help-scroll")
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(palette.text)
                                    .child("支持解析抖音以下链接格式："),
                            )
                            .children(format_rows(&SUPPORTED_FORMATS)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(palette.text)
                                    .child("支持解析小红书笔记（视频 / 图文），以下格式均支持："),
                            )
                            .children(format_rows(&SUPPORTED_XHS_FORMATS)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.0))
                            .child(widgets::gradient_icon(
                                icons::LINK.as_bytes(),
                                16.0,
                                &cx.svg_renderer(),
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .text_size(px(13.0))
                                    .text_color(palette.text_muted)
                                    .line_height(px(20.0))
                                    .whitespace_normal()
                                    .child("支持批量粘贴多个抖音 / 小红书分享链接，系统会自动提取所有有效链接。"),
                            ),
                    ),
            )
            .child(
                self.action_row()
                    .flex_shrink_0()
                    .child(self.text_action(
                        palette,
                        "help-close",
                        "知道了",
                        true,
                        move |_, _, cx| {
                            entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    )),
            )
    }

    fn dialog_about(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity();
        self.dialog_card(palette, width, max_height)
            .child(self.dialog_title(palette, "关于", 24.0).flex_shrink_0())
            .child(
                self.dialog_content_scroll("about-scroll").child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(20.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(palette.text)
                                .child(format!("{APP_NAME}工具")),
                        )
                        .child(
                            div()
                                .text_size(px(15.0))
                                .text_color(palette.text)
                                .child(format!("版本: {APP_VERSION}")),
                        )
                        .child(
                            div()
                                .text_size(px(15.0))
                                .text_color(palette.text)
                                .child("功能: 解析抖音、小红书链接，获取视频下载地址"),
                        )
                        .child(
                            div()
                                .text_size(px(15.0))
                                .text_color(palette.text)
                                .child("注意: 仅用于学习和研究目的"),
                        ),
                ),
            )
            .child(self.action_row().flex_shrink_0().child(self.text_action(
                palette,
                "about-close",
                "确定",
                true,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }

    pub fn open_save_location_dialog(&mut self, cx: &mut Context<Self>) {
        self.location_dialog_directory = if self.settings.last_directory.is_empty() {
            self.paths.default_download_dir.display().to_string()
        } else {
            self.settings.last_directory.clone()
        };
        self.location_dialog_remember =
            self.settings.skip_location_prompt || self.settings.remember_location;
        self.dialog = Some(Dialog::SaveLocation);
        cx.notify();
    }

    pub fn open_xhs_cookie_dialog(&mut self, cx: &mut Context<Self>) {
        next_xhs_cookie_dialog_seq();
        let saved_cookie = self.settings.xhs_cookie.clone();
        self.xhs_cookie_input = self.settings.xhs_cookie.clone();
        reset_xhs_cookie_draft(&saved_cookie);
        self.xhs_cookie_focus = cx.focus_handle();
        self.xhs_cookie_scroll = gpui::ScrollHandle::new();
        self.xhs_cookie_scrollbar_drag = widgets::ScrollbarDrag::default();
        self.xhs_cookie_card_height = 0.0;
        self.xhs_cookie_card_top = None;
        #[cfg(target_os = "android")]
        {
            // The card and the native editor are different compositor
            // surfaces. Mount the editor invisibly for two layout passes,
            // then reveal it and the card on the same frame.
            self.xhs_cookie_warmup_frames = 2;
            self.xhs_cookie_editor_warmup = true;
            self.xhs_cookie_ime_follow = false;
            self.xhs_cookie_ime_preview = false;
            gpui_android::set_native_editor_warmup(true);
            gpui_android::set_native_editor_ime_follow(false);
            gpui_android::set_native_editor_ime_preview(false);
        }
        self.dialog = Some(Dialog::XhsCookie);
        #[cfg(target_os = "android")]
        gpui_android::synchronize_native_editor_session();
        cx.notify();
    }

    /// Discard an unsaved Cookie draft from every close path on every platform.
    pub fn close_xhs_cookie_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        close_or_replace_dialog(self, window, cx);
    }

    fn dialog_xhs_cookie(
        &mut self,
        palette: Palette,
        width: f32,
        max_height: f32,
        landscape: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        #[cfg(not(target_os = "ios"))]
        let input_entity = cx.entity();
        let dialog_sequence = current_xhs_cookie_dialog_seq();
        let cancel_entity = cx.entity();
        let save_entity = cx.entity();
        let input_focused = self.xhs_cookie_focus.is_focused(window);
        // Android's Cookie field is also a single native EditText. Switching
        // between a GPUI-painted field and the native view caused duplicate
        // borders and changed the measured height during focus transitions.
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let native_editor_enabled = true;
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let native_editor_enabled = input_focused || !self.xhs_cookie_input.is_empty();
        #[cfg(target_os = "ios")]
        let input_lines = self
            .xhs_cookie_native_input_lines
            .map(|lines| lines.max(6))
            .unwrap_or(6);
        #[cfg(not(target_os = "ios"))]
        let input_lines = 6;
        let input_box_height = if landscape {
            // In landscape the card is fixed to the window-height constraint
            // passed in as `max_height`, so both the card and this editor use
            // deterministic viewport-derived values from the first frame.
            (max_height / 3.0).max(1.0)
        } else {
            146.0
        };
        #[cfg(target_os = "ios")]
        let input_box_height_value = input_box_height;
        #[cfg(target_os = "ios")]
        let _ = (input_focused, landscape, input_lines, native_editor_enabled);
        #[cfg(not(target_os = "ios"))]
        let input = Input::new(xhs_cookie_input_id())
            .focus_handle(self.xhs_cookie_focus.clone())
            .multiline(true)
            .native_editor_enabled(native_editor_enabled)
            .value(self.xhs_cookie_input.clone())
            .placeholder("请输入小红书 Cookie")
            .size(InputSize::Lg)
            .select_all_on_ctrl(cfg!(target_os = "windows"))
            .min_lines(input_lines)
            .native_editor_min_height(146.0)
            .editor_viewport_height(input_box_height)
            .editor_scroll_handle(self.xhs_cookie_scroll.clone())
            .bg_color(cookie_input_bg(palette.dark))
            .border_color(if cfg!(any(target_os = "android", target_os = "ios")) {
                palette.input_border
            } else {
                gpui::rgba(0x00000000)
            })
            .text_color(palette.text)
            .placeholder_color(palette.hint)
            .on_text_change(move |text, _, cx| {
                input_entity.update(cx, |this, cx| {
                    if current_xhs_cookie_dialog_seq() == dialog_sequence
                        && matches!(this.dialog, Some(Dialog::XhsCookie))
                        && this.xhs_cookie_input != text
                    {
                        this.xhs_cookie_input = text.to_string();
                        cx.notify();
                    }
                });
            });

        // On iOS the GPUI tree contains only an empty layout slot. The Swift
        // UITextView is the sole renderer and draws the native border.
        #[cfg(target_os = "ios")]
        let input_scroll = div()
            .relative()
            .w_full()
            .h(px(input_box_height_value))
            .on_children_prepainted({
                let placeholder_entity = cx.entity();
                move |bounds, window, cx| {
                    let Some(bounds) = bounds.first() else {
                        return;
                    };
                    placeholder_entity.update(cx, |view, cx| {
                        crate::ios_native_input::show_placeholder(
                            XHS_COOKIE_INPUT_ID,
                            &view.xhs_cookie_input,
                            f32::from(bounds.origin.x),
                            f32::from(bounds.origin.y),
                            f32::from(bounds.size.width),
                            f32::from(bounds.size.height),
                            input_box_height_value,
                            146.0,
                            None,
                            palette,
                            view.xhs_cookie_focus.is_focused(window),
                        );
                        let _ = cx;
                    });
                }
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                crate::ios_native_input::focus(XHS_COOKIE_INPUT_ID);
                cx.stop_propagation();
            })
            .child(div().id("xhs-cookie-input-scroll").w_full().h_full());

        #[cfg(not(target_os = "ios"))]
        let mut input_scroll = div()
            .id("xhs-cookie-input-scroll")
            .w_full()
            .max_h(px(input_box_height))
            .overflow_y_scroll()
            .scrollbar_width(px(8.0))
            .track_scroll(&self.xhs_cookie_scroll)
            .text_size(px(16.0))
            .child(div().relative().w_full().child(input));

        #[cfg(target_os = "android")]
        if native_editor_enabled {
            input_scroll = input_scroll.overflow_hidden().scrollbar_width(px(0.0));
        }

        #[cfg(target_os = "android")]
        let ime_preview_visible = native_editor_enabled
            && self.xhs_cookie_ime_preview
            && gpui_android::native_editor_ime_preview_visible()
            && !self.xhs_cookie_editor_warmup
            && native_editor_owned_by(&xhs_cookie_input_id());
        #[cfg(all(not(target_os = "android"), not(target_os = "ios")))]
        let _ime_preview_visible = false;

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let input_scroll = input_scroll.on_mouse_up_out(MouseButton::Left, {
            let focus_handle = self.xhs_cookie_focus.clone();
            let input_bounds = self.xhs_cookie_scroll.bounds();
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

        let scrollbar = if cfg!(any(target_os = "android", target_os = "ios")) {
            div().into_any_element()
        } else {
            widgets::vertical_scrollbar(
                cx,
                "xhs-cookie-input-scrollbar",
                widgets::ScrollbarTarget::CookieInput,
                self.xhs_cookie_scrollbar_drag.clone(),
                self.xhs_cookie_scroll.clone(),
                palette,
            )
        };

        #[cfg(target_os = "ios")]
        let input_box = div().relative().w_full().child(input_scroll);

        #[cfg(not(target_os = "ios"))]
        let mut input_box = div()
            .relative()
            .w_full()
            .max_h(px(input_box_height))
            .overflow_hidden()
            .rounded_md()
            .child(input_scroll)
            .child(scrollbar);

        #[cfg(not(target_os = "ios"))]
        {
            // The native editor is invisible while the card fades in and
            // while the IME animates, so GPUI has to paint the field (chrome
            // and text) for those frames; otherwise the box looks empty.
            #[cfg(target_os = "android")]
            // In landscape the native editor has an opaque GPUI background
            // but no IME preview, so its EditText remains transparent. A
            // permanent GPUI text copy would then show through it as a glyph
            // double image. Keep the copy only for warm-up and portrait IME
            // animation frames, when Java hides the native text on purpose.
            let gpui_paints_cookie_input = native_editor_enabled
                && (!landscape || self.xhs_cookie_editor_warmup || ime_preview_visible);
            #[cfg(not(target_os = "android"))]
            let gpui_paints_cookie_input = false;
            // The native editor paints its own border. Keep an opaque GPUI
            // background and border underneath it so warm-up and IME preview
            // frames still show the field without changing its geometry.
            input_box = input_box.bg(cookie_input_bg(palette.dark)).child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_md()
                    .border_1()
                    .border_color(if input_focused {
                        palette.input_focus
                    } else {
                        palette.input_border
                    }),
            );
            if gpui_paints_cookie_input {
                // The native editor is transparent for the whole IME
                // animation, so GPUI has to show the field's text too. It
                // uses the same font size, line height, and 12dp padding as
                // the input's own text, which the native editor aligns its
                // baseline to, so the hand-off does not move the glyphs.
                let preview_text = if self.xhs_cookie_input.is_empty() {
                    "请输入小红书 Cookie".to_string()
                } else {
                    self.xhs_cookie_input.clone()
                };
                let preview_text_color = if self.xhs_cookie_input.is_empty() {
                    palette.hint
                } else {
                    palette.text
                };
                let scroll_y = f32::from(self.xhs_cookie_scroll.offset().y);
                input_box = input_box.child(
                    div().absolute().inset_0().overflow_hidden().child(
                        div()
                            .absolute()
                            .left(px(12.0))
                            .right(px(12.0))
                            .top(px(12.0 + scroll_y))
                            .text_size(px(16.0))
                            .line_height(px(20.0))
                            .text_color(preview_text_color)
                            .whitespace_normal()
                            .child(preview_text),
                    ),
                );
            }
        }

        let content_scroll = self.dialog_content_scroll("xhs-cookie-content-scroll");
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let content_scroll = content_scroll.flex_initial();

        self.dialog_card(palette, width, max_height)
            .child(
                self.dialog_title(palette, "Cookie 设置", 22.0)
                    .flex_shrink_0(),
            )
            .child(
                content_scroll.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(19.0))
                                .text_color(palette.text_muted)
                                .child("可粘贴浏览器中小红书站点的 Cookie"),
                        )
                        .child(input_box),
                ),
            )
            .child(
                self.action_row()
                    .flex_shrink_0()
                    .child(
                        self.cookie_action("cookie-cancel", "取消", palette.plain_button(), {
                            move |window, cx| {
                                cancel_entity.update(cx, |this, cx| {
                                    #[cfg(not(target_os = "android"))]
                                    let proceed = true;
                                    #[cfg(target_os = "android")]
                                    let proceed =
                                        current_xhs_cookie_dialog_seq() == dialog_sequence;
                                    if proceed {
                                        this.close_xhs_cookie_dialog(window, cx);
                                    }
                                });
                            }
                        }),
                    )
                    .child(self.cookie_action(
                        "cookie-save",
                        "保存",
                        palette.download_button(),
                        {
                            move |window, cx| {
                                save_entity.update(cx, |this, cx| {
                                    #[cfg(not(target_os = "android"))]
                                    let proceed = true;
                                    #[cfg(target_os = "android")]
                                    let proceed =
                                        current_xhs_cookie_dialog_seq() == dialog_sequence;
                                    if !proceed {
                                        return;
                                    }
                                    #[cfg(any(target_os = "android", target_os = "ios"))]
                                    sync_xhs_cookie_editor_text(this);
                                    this.settings.xhs_cookie =
                                        this.xhs_cookie_input.trim().to_string();
                                    this.save_settings();
                                    this.close_xhs_cookie_dialog(window, cx);
                                    this.toast("Cookie 设置已保存", ToastKind::Success, cx);
                                });
                            }
                        },
                    )),
            )
    }

    fn dialog_save_location(
        &mut self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let current = if self.location_dialog_directory.is_empty() {
            "未设置".to_string()
        } else {
            self.location_dialog_directory.clone()
        };
        let remember = self.location_dialog_remember;
        let change_entity = cx.entity();
        let reset_entity = cx.entity();
        let toggle_entity = cx.entity();
        let cancel_entity = cx.entity();
        let save_entity = cx.entity();

        let checkbox = div()
            .w(px(20.0))
            .h(px(20.0))
            .rounded(px(3.0))
            .border_1()
            .border_color(if remember {
                palette.primary
            } else {
                palette.text_muted
            })
            .bg(if remember {
                palette.primary
            } else {
                gpui::rgba(0x00000000)
            })
            .flex()
            .items_center()
            .justify_center()
            .when(remember, |el| {
                el.child(widgets::icon(icons::CHECK, 14.0, gpui::rgb(0xffffff)))
            });

        let content = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(div().text_size(px(14.0)).child("当前保存位置:"))
                    .child(
                        div()
                            .w_full()
                            .p(px(6.0))
                            .rounded(px(6.0))
                            .bg(palette.path_bg)
                            .border_1()
                            .border_color(if palette.dark {
                                rgb(0x4c4d4f)
                            } else {
                                rgb(0xdcdfe6)
                            })
                            .flex()
                            .items_center()
                            .justify_start()
                            .gap(px(4.0))
                            .child(widgets::icon(icons::FOLDER, 16.0, palette.text_muted))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .text_size(px(13.0))
                                    .line_height(px(19.0))
                                    .text_color(palette.text_muted)
                                    .text_left()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(current),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_start()
                    .gap(px(12.0))
                    .child(self.storage_action(
                        palette,
                        "location-change",
                        "更改",
                        move |_, _, cx| {
                            change_entity.update(cx, |this, cx| {
                                let initial = if this.location_dialog_directory.is_empty() {
                                    None
                                } else {
                                    Some(PathBuf::from(&this.location_dialog_directory))
                                };
                                cx.spawn(async move |weak: WeakEntity<AppView>, cx| {
                                    let picked =
                                        crate::home::pick_directory_blocking(initial.as_deref());
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Some(directory) = picked {
                                            this.location_dialog_directory =
                                                directory.display().to_string();
                                            cx.notify();
                                        }
                                    });
                                })
                                .detach();
                            });
                        },
                    ))
                    .child(self.storage_reset_action(
                        palette,
                        "location-reset",
                        "重置",
                        move |_, _, cx| {
                            reset_entity.update(cx, |this, cx| {
                                this.location_dialog_directory =
                                    this.paths.default_download_dir.display().to_string();
                                this.location_dialog_remember = false;
                                cx.notify();
                            });
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(div().text_size(px(14.0)).child("下载选项:"))
                    .child(
                        div()
                            .id("location-remember-row")
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .rounded_full()
                            .cursor_pointer()
                            .px(px(4.0))
                            .py(px(4.0))
                            .on_click(move |_, _, cx| {
                                toggle_entity.update(cx, |this, cx| {
                                    this.location_dialog_remember = !this.location_dialog_remember;
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(palette.text)
                                            .child("不再提示保存位置"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(palette.text_muted)
                                            .child("下次直接保存到此位置"),
                                    ),
                            )
                            .child(checkbox),
                    ),
            );

        self.dialog_card(palette, width, max_height)
            .child(self.dialog_title(palette, "存储设置", 22.0).flex_shrink_0())
            .child(
                self.dialog_content_scroll("location-content-scroll")
                    .child(content),
            )
            .child(
                self.action_row()
                    .flex_shrink_0()
                    .child(self.text_action(
                        palette,
                        "location-cancel",
                        "取消",
                        false,
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "location-save",
                        "保存",
                        move |_, _, cx| {
                            save_entity.update(cx, |this, cx| {
                                let directory = this.location_dialog_directory.trim().to_string();
                                if directory.is_empty() {
                                    this.toast("请先选择保存位置", ToastKind::Error, cx);
                                    return;
                                }
                                this.settings.last_directory = directory;
                                this.settings.skip_location_prompt = this.location_dialog_remember;
                                this.settings.remember_location = this.location_dialog_remember;
                                this.save_settings();
                                this.dialog = None;
                                this.toast("设置已保存", ToastKind::Success, cx);
                            });
                        },
                    )),
            )
    }

    fn dialog_clear_cache(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let cancel_entity = cx.entity();
        let clear_entity = cx.entity();
        let cache_size = format_size(self.cache_size);
        let paths: Vec<AnyElement> = self
            .cache_paths
            .iter()
            .map(|path| {
                div()
                    .text_size(px(11.0))
                    .text_color(palette.text_muted)
                    .child(format!("• {path}"))
                    .into_any_element()
            })
            .collect();

        self.dialog_card(palette, width, max_height)
            .child(self.dialog_title(palette, "清除缓存", 20.0).flex_shrink_0())
            .child(
                self.dialog_content_scroll("clear-cache-content-scroll")
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .child("确定要清除所有缓存数据吗？"),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(palette.text_secondary)
                                    .child(format!("当前缓存大小: {cache_size}")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(palette.text_muted)
                                    .child("将清理以下位置的缓存:"),
                            )
                            .children(paths),
                    ),
            )
            .child(
                self.action_row()
                    .flex_shrink_0()
                    .child(self.text_action(
                        palette,
                        "cache-cancel",
                        "取消",
                        false,
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "cache-clear",
                        "清理",
                        move |_, _, cx| {
                            clear_entity.update(cx, |this, cx| this.clear_cache(cx));
                        },
                    )),
            )
    }

    fn dialog_no_cache(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity();
        self.dialog_card(palette, width, max_height)
            .child(self.dialog_title(palette, "暂无缓存", 20.0).flex_shrink_0())
            .child(
                self.dialog_content_scroll("no-cache-content-scroll").child(
                    div()
                        .text_size(px(14.0))
                        .text_color(palette.text_secondary)
                        .child("当前没有可清理的缓存，无需清理"),
                ),
            )
            .child(self.action_row().flex_shrink_0().child(self.text_action(
                palette,
                "no-cache-ok",
                "知道了",
                true,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }

    fn dialog_clear_result(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        success: bool,
        message: String,
        details: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity();
        let detail_elements: Vec<AnyElement> = details
            .iter()
            .map(|detail| {
                let color = if detail.contains('❌') {
                    gpui::rgb(0xd32f2f)
                } else if detail.contains('🗑') {
                    gpui::rgb(0x05c001)
                } else {
                    palette.text_muted
                };
                div()
                    .text_size(px(12.0))
                    .text_color(color)
                    .child(detail.clone())
                    .into_any_element()
            })
            .collect();

        self.dialog_card(palette, width, max_height)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(widgets::icon(
                        if success {
                            icons::CHECK_CIRCLE
                        } else {
                            icons::INFO
                        },
                        24.0,
                        if success {
                            gpui::rgb(0x05c001)
                        } else {
                            gpui::rgb(0xd32f2f)
                        },
                    ))
                    .child(self.dialog_title(
                        palette,
                        if success {
                            "清理完成"
                        } else {
                            "清理失败"
                        },
                        20.0,
                    )),
            )
            .child(
                self.dialog_content_scroll("clear-result-content-scroll")
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if success {
                                gpui::rgb(0x05c001)
                            } else {
                                gpui::rgb(0xd32f2f)
                            })
                            .child(message),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .children(detail_elements),
                    ),
            )
            .child(self.action_row().flex_shrink_0().child(self.text_action(
                palette,
                "clean-ok",
                "确定",
                true,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }

    fn dialog_download_selection(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let cancel_entity = cx.entity();
        let close_entity = cx.entity();
        let all_entity = cx.entity();
        let original_entity = cx.entity();
        let mut title_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .child(self.dialog_title(palette, "下载资源", 20.0));
        if !is_desktop() {
            title_row = title_row.child(
                div()
                    .id("download-selection-close")
                    .w(px(28.0))
                    .h(px(28.0))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |style| style.bg(palette.window_btn_hover))
                    .on_click(move |_, _, cx| {
                        close_entity.update(cx, |this, cx| {
                            this.dialog = None;
                            cx.notify();
                        });
                    })
                    .child(widgets::gradient_icon(
                        icons::CLOSE_SMALL.as_bytes(),
                        18.0,
                        &cx.svg_renderer(),
                    )),
            );
        }

        let mut actions = self.action_row().flex_shrink_0();
        if is_desktop() {
            actions = actions.child(self.plain_action(
                palette,
                "download-cancel",
                "取消",
                move |_, _, cx| {
                    cancel_entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            ));
        }
        actions = actions
            .child(
                self.outline_action(palette, "download-all", "下载全部", move |_, _, cx| {
                    all_entity.update(cx, |this, cx| this.confirm_download(ResourceScope::All, cx));
                }),
            )
            .child(self.primary_action(
                palette,
                "download-original",
                "下载原画",
                move |_, _, cx| {
                    original_entity.update(cx, |this, cx| {
                        this.confirm_download(ResourceScope::Original, cx)
                    });
                },
            ));

        self.dialog_card(palette, width, max_height)
            .child(title_row)
            .child(
                self.dialog_content_scroll("download-selection-content-scroll")
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(22.0))
                            .text_color(palette.text)
                            .child("请选择要下载的资源范围：原画仅保留视频中的原画条目。"),
                    ),
            )
            .child(actions)
    }

    fn dialog_live_photo_export(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let normal_entity = cx.entity();
        let live_entity = cx.entity();
        let cancel_entity = cx.entity();
        let confirm_entity = cx.entity();
        let live_selected = self.live_photo_dialog_choice;

        let header = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .flex_shrink_0()
            .child(self.dialog_title(palette, "导出格式", 20.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(palette.text_secondary)
                    .child("选择默认的导出方式"),
            );

        let normal_row = self.live_photo_option(
            palette,
            "live-photo-option-normal",
            "普通视频 / 图片",
            "按照原始格式保存",
            !live_selected,
            move |_, _, cx| {
                normal_entity.update(cx, |this, cx| {
                    this.live_photo_dialog_choice = false;
                    cx.notify();
                });
            },
        );
        let live_row = self.live_photo_option(
            palette,
            "live-photo-option-live",
            "实况照片（Live Photo）",
            "保存为 iPhone 实况照片",
            live_selected,
            move |_, _, cx| {
                live_entity.update(cx, |this, cx| {
                    this.live_photo_dialog_choice = true;
                    cx.notify();
                });
            },
        );

        let actions = self
            .action_row()
            .flex_shrink_0()
            .child(
                self.plain_action(palette, "live-photo-cancel", "取消", move |_, _, cx| {
                    cancel_entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                }),
            )
            .child(self.primary_action(
                palette,
                "live-photo-confirm",
                "确定",
                move |_, _, cx| {
                    confirm_entity.update(cx, |this, cx| {
                        this.settings.live_photo_export = this.live_photo_dialog_choice;
                        this.save_settings();
                        this.dialog = None;
                        let message = if this.settings.live_photo_export {
                            "导出到相册时将封装为实况照片"
                        } else {
                            "导出到相册时将保存为图片和视频"
                        };
                        this.toast(message, ToastKind::Info, cx);
                        cx.notify();
                    });
                },
            ));

        self.dialog_card(palette, width, max_height)
            .child(header)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(normal_row)
                    .child(live_row),
            )
            .child(actions)
    }

    fn live_photo_option(
        &self,
        palette: Palette,
        id: &'static str,
        title: &str,
        subtitle: &str,
        selected: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let indicator = if selected {
            div()
                .w(px(22.0))
                .h(px(22.0))
                .flex_shrink_0()
                .rounded(px(6.0))
                .bg(palette.primary)
                .flex()
                .items_center()
                .justify_center()
                .child(widgets::icon(icons::CHECK_MARK, 14.0, gpui::rgb(0xffffff)))
                .into_any_element()
        } else {
            div()
                .w(px(22.0))
                .h(px(22.0))
                .flex_shrink_0()
                .rounded(px(6.0))
                .border_1()
                .border_color(palette.text_muted)
                .into_any_element()
        };
        div()
            .id(id)
            .w_full()
            .py(px(12.0))
            .rounded(px(12.0))
            .flex()
            .items_center()
            .gap(px(12.0))
            .cursor_pointer()
            .on_click(on_click)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(17.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text)
                            .child(title.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(palette.text_secondary)
                            .child(subtitle.to_string()),
                    ),
            )
            .child(indicator)
            .into_any_element()
    }

    #[cfg(target_os = "windows")]
    fn dialog_export_method(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let script_entity = cx.entity();
        let idm_entity = cx.entity();
        let cancel_entity = cx.entity();
        self.dialog_card(palette, width, max_height)
            .child(
                self.dialog_title(palette, "导出与下载", 20.0)
                    .flex_shrink_0(),
            )
            .child(
                self.dialog_content_scroll("export-method-content-scroll")
                    .child(
                        dialog_text_line(palette.text)
                            .child(plain_text(
                                "请根据需求选择导出的方式，支持导出下载脚本和使用 ",
                            ))
                            .child(link_text(
                                "export-method-idm-homepage-link",
                                "Internet Download Manager (IDM) ",
                                IDM_HOMEPAGE_URL,
                                palette,
                            ))
                            .child(plain_text("下载")),
                    ),
            )
            .child(
                self.action_row()
                    .flex_shrink_0()
                    .child(self.plain_action(
                        palette,
                        "export-method-cancel",
                        "取消",
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.outline_action(
                        palette,
                        "export-method-script",
                        "导出下载脚本",
                        move |_, _, cx| {
                            script_entity.update(cx, |this, cx| this.choose_export_scripts(cx));
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "export-method-idm",
                        "导出到idm",
                        move |_, _, cx| {
                            idm_entity.update(cx, |this, cx| this.request_idm_export(cx));
                        },
                    )),
            )
    }

    #[cfg(target_os = "windows")]
    fn dialog_idm_selection(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let cancel_entity = cx.entity();
        let all_entity = cx.entity();
        let original_entity = cx.entity();
        self.dialog_card(palette, width, max_height)
            .child(
                self.dialog_title(palette, "导出到idm", 20.0)
                    .flex_shrink_0(),
            )
            .child(
                self.dialog_content_scroll("idm-selection-content-scroll")
                    .child(
                        dialog_text_line(palette.text)
                            .child(plain_text("请选择要导出到 "))
                            .child(link_text(
                                "idm-selection-homepage-link",
                                "IDM",
                                IDM_HOMEPAGE_URL,
                                palette,
                            ))
                            .child(plain_text(" 的资源范围：原画仅保留视频中的原画条目")),
                    ),
            )
            .child(
                self.action_row()
                    .flex_shrink_0()
                    .child(
                        self.plain_action(palette, "idm-cancel", "取消", move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        }),
                    )
                    .child(self.outline_action(
                        palette,
                        "idm-all",
                        "导出全部",
                        move |_, _, cx| {
                            all_entity.update(cx, |this, cx| {
                                this.confirm_idm_export(ResourceScope::All, cx)
                            });
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "idm-original",
                        "导出原画",
                        move |_, _, cx| {
                            original_entity.update(cx, |this, cx| {
                                this.confirm_idm_export(ResourceScope::Original, cx)
                            });
                        },
                    )),
            )
    }

    #[cfg(target_os = "windows")]
    fn dialog_idm_result(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        success: bool,
        message: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity();
        self.dialog_card(palette, width, max_height)
            .child(
                self.dialog_title(
                    palette,
                    if success {
                        "成功导出到idm"
                    } else {
                        "导出失败"
                    },
                    20.0,
                )
                .flex_shrink_0(),
            )
            .child(
                self.dialog_content_scroll("idm-result-content-scroll")
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(22.0))
                            .text_color(palette.text)
                            .child(if success {
                                "链接已成功发送到 IDM。".to_string()
                            } else {
                                format!("无法发送到 IDM：{message}")
                            }),
                    ),
            )
            .child(self.action_row().flex_shrink_0().child(self.text_action(
                palette,
                "idm-result-ok",
                "确定",
                true,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }

    fn dialog_export_selection(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let cancel_entity = cx.entity();
        let all_entity = cx.entity();
        let original_entity = cx.entity();

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let export_selection_content = dialog_text_line(palette.text)
            .child(plain_text(
                "请选择要导出的资源范围：原画仅导出视频中的原画条目，支持 ",
            ))
            .child(link_text(
                "export-selection-aria2c-link",
                "aria2c",
                ARIA2C_URL,
                palette,
            ))
            .child(plain_text("、"))
            .child(link_text(
                "export-selection-downloader-link",
                "downloader",
                DOWNLOADER_URL,
                palette,
            ))
            .child(plain_text("、"))
            .child(link_text(
                "export-selection-robust-downloader-link",
                "RobustDownloader",
                ROBUST_DOWNLOADER_URL,
                palette,
            ))
            .child(plain_text(" 下载脚本"))
            .into_any_element();

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let export_selection_content = div()
            .text_size(px(14.0))
            .line_height(px(22.0))
            .text_color(palette.text)
            .child("请选择要导出的资源范围：原画仅导出视频中的原画条目，支持 aria2c、downloader、RobustDownloader 下载脚本")
            .into_any_element();

        self.dialog_card(palette, width, max_height)
            .child(
                self.dialog_title(palette, "导出下载脚本", 20.0)
                    .flex_shrink_0(),
            )
            .child(
                self.dialog_content_scroll("export-selection-content-scroll")
                    .child(export_selection_content),
            )
            .child(
                self.action_row()
                    .flex_shrink_0()
                    .child(self.plain_action(
                        palette,
                        "export-cancel",
                        "取消",
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.outline_action(
                        palette,
                        "export-all",
                        "导出全部",
                        move |_, _, cx| {
                            all_entity.update(cx, |this, cx| {
                                this.confirm_export_scripts(ResourceScope::All, cx)
                            });
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "export-original",
                        "导出原画",
                        move |_, _, cx| {
                            original_entity.update(cx, |this, cx| {
                                this.confirm_export_scripts(ResourceScope::Original, cx)
                            });
                        },
                    )),
            )
    }

    fn dialog_download_progress(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let close_entity = cx.entity();
        let (completed, total, overall, current_index, current_progress, downloaded, total_size) =
            if let Some(ui) = self.download.as_ref() {
                (
                    ui.completed_count(),
                    ui.total,
                    ui.overall_progress(),
                    ui.current_index().unwrap_or(0),
                    ui.current_progress(),
                    ui.total_downloaded(),
                    ui.total_size(),
                )
            } else {
                (0, 0, 0.0, 0, 0.0, 0, 0)
            };
        let all_completed = total > 0 && completed == total;
        let status = if all_completed {
            "所有下载完成！正在关闭进度窗口..."
        } else {
            "正在顺序下载..."
        };
        let current_size = self
            .download
            .as_ref()
            .and_then(|ui| ui.downloaded.get(current_index).copied())
            .unwrap_or(0);
        let current_total = self
            .download
            .as_ref()
            .and_then(|ui| ui.total_bytes.get(current_index).copied())
            .unwrap_or(0);

        self.dialog_card(palette, width, max_height)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_shrink_0()
                    .child(self.dialog_title(palette, "下载进度", 20.0))
                    .child(
                        div()
                            .id("progress-close")
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |style| style.bg(palette.window_btn_hover))
                            .on_click(move |_, _, cx| {
                                close_entity.update(cx, |this, cx| this.cancel_download(cx));
                            })
                            .child(widgets::gradient_icon(
                                icons::CLOSE_SMALL.as_bytes(),
                                18.0,
                                &cx.svg_renderer(),
                            )),
                    ),
            )
            .child(
                self.dialog_content_scroll("download-progress-content-scroll")
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(palette.text)
                                            .child("总体进度:"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(gpui::rgb(0x05c001))
                                            .child(format!("{completed}/{total} 已完成")),
                                    ),
                            )
                            .child(ProgressBar {
                                value: overall,
                                color: palette.primary,
                                track: palette.divider,
                                height: 8.0,
                            })
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::BOLD)
                                            .child(format!("{:.1}%", overall * 100.0)),
                                    )
                                    .child(div().text_size(px(12.0)).child(format!(
                                        "{} / {}",
                                        format_size(downloaded),
                                        if total_size > 0 {
                                            format_size(total_size)
                                        } else {
                                            "计算中...".to_string()
                                        }
                                    ))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(div().text_size(px(12.0)).child(format!(
                                "当前视频 ({}/{}):",
                                current_index + 1,
                                total
                            )))
                            .child(ProgressBar {
                                value: current_progress,
                                color: palette.primary,
                                track: palette.divider,
                                height: 8.0,
                            })
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::BOLD)
                                            .child(format!("{:.1}%", current_progress * 100.0)),
                                    )
                                    .child(div().text_size(px(12.0)).child(format!(
                                        "{} / {}",
                                        format_size(current_size),
                                        format_size(current_total.max(current_size))
                                    ))),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(if all_completed {
                                gpui::rgb(0x05c001)
                            } else {
                                palette.text_muted
                            })
                            .child(status),
                    ),
            )
    }

    fn dialog_download_summary(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        files: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Div {
        let close_entity = cx.entity();
        let first_directory = files
            .first()
            .and_then(|file| file.parent().map(|p| p.to_path_buf()));
        let mut entries: Vec<AnyElement> = files
            .iter()
            .take(5)
            .map(|file| {
                let name = file
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                div()
                    .text_size(px(12.0))
                    .text_color(palette.text)
                    .child(format!("• {name}"))
                    .into_any_element()
            })
            .collect();
        if files.len() > 5 {
            entries.push(
                div()
                    .text_size(px(12.0))
                    .text_color(palette.text_muted)
                    .child(format!("... 以及 {} 个文件", files.len() - 5))
                    .into_any_element(),
            );
        }

        let card = self
            .dialog_card(palette, width, max_height)
            .child(self.dialog_title(palette, "下载完成", 20.0).flex_shrink_0())
            .child(
                self.dialog_content_scroll("download-summary-content-scroll")
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("已成功下载 {} 个视频文件：", files.len())),
                    )
                    .child(div().flex().flex_col().gap(px(4.0)).children(entries)),
            );

        let mut actions = self.action_row().child(self.plain_action(
            palette,
            "summary-close",
            "关闭",
            move |_, _, cx| {
                close_entity.update(cx, |this, cx| {
                    this.dialog = None;
                    cx.notify();
                });
            },
        ));

        if is_desktop() {
            if let Some(directory) = first_directory {
                actions = actions.child(self.primary_action(
                    palette,
                    "summary-open",
                    "打开文件夹",
                    move |_, _, _| {
                        open_directory(&directory);
                    },
                ));
            }
        }

        card.child(actions.flex_shrink_0())
    }

    fn dialog_download_saved(
        &self,
        palette: Palette,
        width: f32,
        max_height: f32,
        count: usize,
        failed: usize,
        skipped: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity();
        let summary = if failed > 0 {
            format!("{count} 个资源已保存到系统相册，{failed} 个下载失败")
        } else {
            format!("{count} 个资源已保存到系统相册")
        };
        let mut content = self
            .dialog_content_scroll("download-saved-content-scroll")
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(22.0))
                    .text_color(palette.text)
                    .child(summary),
            );
        if skipped > 0 {
            content = content.child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(22.0))
                    .text_color(palette.text_secondary)
                    .child(format!("已跳过 {skipped} 个 MP3 文件（相册不支持音频）")),
            );
        }
        self.dialog_card(palette, width, max_height)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(widgets::icon(
                        icons::CHECK_CIRCLE,
                        24.0,
                        gpui::rgb(0x05c001),
                    ))
                    .child(self.dialog_title(palette, "下载完成", 20.0)),
            )
            .child(content)
            .child(self.action_row().flex_shrink_0().child(self.primary_action(
                palette,
                "download-saved-ok",
                "知道了",
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }
}
