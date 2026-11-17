//! 短视频解析 GPUI 界面。

pub mod dialogs;
pub mod download;
pub mod download_flow;
mod download_icon;
mod download_sound;
pub mod gallery;
pub mod home;
pub mod icons;
#[cfg(target_os = "windows")]
mod idm;
#[cfg(target_os = "ios")]
mod ios_native_input;
pub mod paths;
pub mod settings_screen;
pub mod theme;
pub mod titlebar;
pub mod widgets;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use douyin_input::TextContextMenuAction;
#[cfg(target_os = "macos")]
pub use douyin_input::{
    dispatch_mac_text_context_menu_action, set_mac_text_context_menu_request_hook,
    MacTextContextMenuRequest, MacTextContextMenuRequestHook,
};
#[cfg(target_os = "windows")]
pub use douyin_input::{set_text_context_menu_hook, TextContextMenuHook, TextContextMenuRequest};
pub use download_icon::set_download_icon_progress_hook;
pub use download_sound::set_download_sound_hook;
#[cfg(target_os = "windows")]
pub use idm::{set_idm_export_hook, IdmLink};

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use douyin_core::extractor::{MediaEntry, MediaKind, MediaPlatform};
use douyin_core::settings::AppSettings;
use gpui::{
    div, px, size, AnyElement, App, AppContext, Bounds, Context, FocusHandle, InteractiveElement,
    IntoElement, Menu, MenuItem, MouseButton, OsAction, ParentElement, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, Subscription, TitlebarOptions, WeakEntity,
    Window, WindowAppearance, WindowBounds, WindowKind, WindowOptions,
};
use gpui_ui_kit::theme::{ThemeState, ThemeVariant};

use crate::download::DownloadEvent;
use crate::paths::{is_desktop, AppPaths};
use crate::theme::Palette;

pub const APP_NAME: &str = "短视频解析";
pub const APP_VERSION: &str = match option_env!("APP_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
pub const SYSTEM_BAR_HEIGHT: f32 = 32.0;
pub const TITLE_ROW_HEIGHT: f32 = 44.0;
pub const MOBILE_APPBAR_HEIGHT: f32 = 56.0;

gpui::actions!(
    douyin_ui,
    [
        ExitFullscreen,
        ToggleFullscreen,
        About,
        Help,
        Minimize,
        Zoom,
        Quit,
        PageBack
    ]
);

#[derive(Clone, Debug)]
pub struct ResultItem {
    pub title: String,
    pub platform: MediaPlatform,
    pub media_kind: MediaKind,
    pub entries: Vec<MediaEntry>,
}

impl ResultItem {
    /// 全部链接数量。
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// 主条目（原画/最佳画质优先）。
    pub fn primary_entry(&self) -> Option<&MediaEntry> {
        self.entries.first()
    }

    /// 按范围选择资源；图片全部保留，视频只保留首选条目。
    /// 图文 / 实况里每个视频对应不同的图片，不做画质去重。
    pub fn select_entries<'a>(&'a self, scope: ResourceScope) -> Vec<&'a MediaEntry> {
        if scope == ResourceScope::All || self.media_kind == MediaKind::Image {
            return self.entries.iter().collect();
        }

        let mut kept_video = false;
        self.entries
            .iter()
            .filter(|entry| {
                if Self::is_video_entry(entry) {
                    if kept_video {
                        false
                    } else {
                        kept_video = true;
                        true
                    }
                } else {
                    true
                }
            })
            .collect()
    }

    pub fn is_video_entry(entry: &MediaEntry) -> bool {
        let extension = entry
            .extension
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let url = entry.url.to_ascii_lowercase();
        matches!(extension.as_str(), "mp4" | "m4v" | "mov" | "webm")
            || url.ends_with(".mp4")
            || url.ends_with(".m4v")
            || url.ends_with(".mov")
            || url.ends_with(".webm")
            || url.contains("/video/")
            || (url.contains("/stream/") && url.contains(".mp4"))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Home,
    Results,
    Settings,
}

const PAGE_TRANSITION_DURATION: Duration = Duration::from_millis(280);
const EDGE_BACK_THRESHOLD: f32 = 0.24;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageTransitionKind {
    Push,
    Pop,
}

/// Shared horizontal travel for the page animation and the native editor
/// clip. Both sides must use this exact value in the same frame.
///
/// `Push` starts one full screen-width to the right and moves left to 0;
/// `Pop` starts at 0 and moves right to one full screen-width.
pub(crate) fn transition_slide_offset(
    viewport_width: f32,
    kind: PageTransitionKind,
    progress: f32,
) -> f32 {
    match kind {
        PageTransitionKind::Push => viewport_width * (1.0 - progress),
        PageTransitionKind::Pop => viewport_width * progress,
    }
}

pub struct PageAnimation {
    started_at: Instant,
    from_progress: f32,
    to_progress: f32,
}

pub struct PageTransition {
    pub from: Route,
    pub to: Route,
    pub kind: PageTransitionKind,
    pub progress: f32,
    pub dragging: bool,
    animation: Option<PageAnimation>,
}

#[cfg(target_os = "macos")]
struct MacWindowClosedGuard {
    _window_closed: Subscription,
}

#[cfg(target_os = "macos")]
impl gpui::Global for MacWindowClosedGuard {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub seq: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResourceScope {
    All,
    Original,
}

#[derive(Clone)]
pub enum Dialog {
    Help,
    About,
    SaveLocation,
    XhsCookie,
    LivePhotoExport,
    ClearCache,
    ClearCacheResult {
        success: bool,
        message: String,
        details: Vec<String>,
    },
    DownloadSelection,
    #[cfg(target_os = "windows")]
    ExportMethod,
    ExportSelection,
    #[cfg(target_os = "windows")]
    IdmSelection,
    #[cfg(target_os = "windows")]
    IdmResult {
        success: bool,
        message: String,
    },
    DownloadProgress,
    DownloadSaved {
        count: usize,
        failed: usize,
        skipped: usize,
    },
    DownloadSummary {
        files: Vec<PathBuf>,
    },
}

pub struct DownloadUi {
    pub total: usize,
    pub current: usize,
    pub completed: Vec<bool>,
    pub downloaded: Vec<u64>,
    pub total_bytes: Vec<u64>,
    pub files: Vec<PathBuf>,
    pub errors: Vec<String>,
    /// 手机端跳过的音频条目数（相册不支持 MP3）。
    pub skipped_audio: usize,
    pub finished: bool,
    pub cancel: Arc<AtomicBool>,
}

impl DownloadUi {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            current: 0,
            completed: vec![false; total],
            downloaded: vec![0; total],
            total_bytes: vec![0; total],
            files: Vec::new(),
            errors: Vec::new(),
            skipped_audio: 0,
            finished: false,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn completed_count(&self) -> usize {
        self.completed.iter().filter(|done| **done).count()
    }

    pub fn current_index(&self) -> Option<usize> {
        if self.total == 0 {
            return None;
        }
        Some(self.current.min(self.total - 1))
    }

    pub fn overall_progress(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        let mut progress = self.completed_count() as f32 / self.total as f32;
        if let Some(index) = self.current_index() {
            if !self.completed.get(index).copied().unwrap_or(false)
                && self.total_bytes.get(index).copied().unwrap_or(0) > 0
            {
                progress += (self.downloaded[index] as f32 / self.total_bytes[index] as f32)
                    / self.total as f32;
            }
        }
        progress.clamp(0.0, 1.0)
    }

    pub fn current_progress(&self) -> f32 {
        let Some(index) = self.current_index() else {
            return 0.0;
        };
        if self.completed.get(index).copied().unwrap_or(false) {
            return 1.0;
        }
        let total = self.total_bytes.get(index).copied().unwrap_or(0);
        if total == 0 {
            0.0
        } else {
            (self.downloaded[index] as f32 / total as f32).clamp(0.0, 1.0)
        }
    }

    pub fn total_downloaded(&self) -> u64 {
        self.downloaded.iter().sum()
    }

    pub fn total_size(&self) -> u64 {
        self.total_bytes
            .iter()
            .enumerate()
            .map(|(index, total)| {
                if *total > 0 {
                    *total
                } else if self.completed.get(index).copied().unwrap_or(false) {
                    self.downloaded[index]
                } else {
                    0
                }
            })
            .sum()
    }
}

pub struct AppView {
    pub paths: AppPaths,
    pub settings: AppSettings,
    pub system_dark: bool,
    pub route: Route,
    pub transition: Option<PageTransition>,
    pub transition_seq: u64,
    pub download_scope: ResourceScope,
    pub input: String,
    pub input_focus: FocusHandle,
    pub input_scroll: ScrollHandle,
    pub input_scroll_metrics: (f32, f32),
    pub input_scrollbar_drag: widgets::ScrollbarDrag,
    pub home_scroll: ScrollHandle,
    pub last_visible_bottom: f32,
    pub last_home_input_focus: bool,
    pub last_home_ime_visible: bool,
    /// Highest input-height limit observed while the IME was visible. Keep
    /// that limit after the keyboard closes so the retained draft uses the
    /// keyboard-sized layout instead of expanding to the full screen.
    pub home_input_ime_line_cap: Option<usize>,
    /// Effective line cap used by the last home-input layout pass.
    pub home_input_effective_line_cap: usize,
    /// Android rotation must not turn a landscape-sized editor into a much
    /// taller portrait one. This cap persists across rotations until the user
    /// starts a new input-focus session.
    pub home_input_rotation_line_cap: Option<usize>,
    /// Current layout width used to detect portrait/landscape rotation.
    pub home_input_layout_width: f32,
    /// Last visible viewport height used to recompute the input cap on rotation.
    pub home_input_layout_height: f32,
    /// Visual line count measured by the iOS native text view for its
    /// current text. Used as the authoritative field height.
    pub home_native_input_lines: Option<usize>,
    pub xhs_cookie_focus: FocusHandle,
    pub xhs_cookie_scroll: ScrollHandle,
    pub xhs_cookie_scrollbar_drag: widgets::ScrollbarDrag,
    /// Actual prepainted Cookie card height. Android centers the card in the
    /// screen area left above the keyboard, with its top clamped to zero.
    pub xhs_cookie_card_height: f32,
    /// Last prepainted Cookie card top, used to translate its separate native
    /// editor window by the same IME-induced jump in one frame.
    pub xhs_cookie_card_top: Option<f32>,
    /// Whether Android is moving the Cookie editor from its IME inset callback.
    pub xhs_cookie_ime_follow: bool,
    /// Android-only: GPUI paints the Cookie input while the native editor is
    /// hidden for an IME animation.
    #[cfg(target_os = "android")]
    pub xhs_cookie_ime_preview: bool,
    /// Android's Cookie editor is a separate WindowManager surface. Keep the
    /// card transparent while Java mounts that window and then reveal both
    /// surfaces together.
    #[cfg(target_os = "android")]
    pub xhs_cookie_warmup_frames: u8,
    /// Whether the Android Cookie editor is still being mounted invisibly.
    #[cfg(target_os = "android")]
    pub xhs_cookie_editor_warmup: bool,
    /// Visual line count measured by the native Cookie editor.
    pub xhs_cookie_native_input_lines: Option<usize>,
    pub results_scroll: ScrollHandle,
    pub loading: bool,
    pub spinner_phase: f32,
    pub results: Vec<ResultItem>,
    pub toast: Option<Toast>,
    pub toast_seq: u64,
    pub dialog: Option<Dialog>,
    pub cache_size: u64,
    pub cache_paths: Vec<String>,
    pub download: Option<DownloadUi>,
    pub download_rx: Option<Receiver<DownloadEvent>>,
    pub location_dialog_directory: String,
    pub location_dialog_remember: bool,
    /// 实况照片弹窗的草稿选择；点“确定”后才写入设置。
    pub live_photo_dialog_choice: bool,
    pub xhs_cookie_input: String,
    _appearance_sub: Option<Subscription>,
    #[cfg(target_os = "android")]
    android_back_pressed_at: Option<Instant>,
}

impl AppView {
    pub fn new(
        paths: AppPaths,
        settings: AppSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let system_dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let weak = cx.entity().downgrade();
        let _appearance_sub = Some(window.observe_window_appearance(move |window, cx| {
            let dark = matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            );
            let _ = weak.update(cx, |this, cx| {
                if this.system_dark != dark {
                    this.system_dark = dark;
                    crate::apply_theme(cx, this.dark());
                    cx.notify();
                }
            });
        }));
        let input_focus = cx.focus_handle();
        #[cfg(target_os = "android")]
        if let Some(window_handle) = window.window_handle().downcast::<AppView>() {
            let async_cx = cx.to_async();
            gpui_android::set_back_handler(Some(Box::new(move || {
                let async_cx = async_cx.clone();
                async_cx
                    .spawn(async move |cx| {
                        let _ = window_handle.update(cx, |view, window, cx| {
                            view.handle_android_system_back(window, cx);
                        });
                    })
                    .detach();
            })));
        }
        let mut view = Self {
            paths,
            settings,
            system_dark,
            route: Route::Home,
            transition: None,
            transition_seq: 0,
            download_scope: ResourceScope::All,
            input: String::new(),
            input_focus: input_focus.clone(),
            input_scroll: ScrollHandle::new(),
            input_scroll_metrics: (0.0, 0.0),
            input_scrollbar_drag: widgets::ScrollbarDrag::default(),
            home_scroll: ScrollHandle::new(),
            last_visible_bottom: 0.0,
            last_home_input_focus: false,
            last_home_ime_visible: false,
            home_input_ime_line_cap: None,
            home_input_effective_line_cap: 0,
            home_input_rotation_line_cap: None,
            home_input_layout_width: 0.0,
            home_input_layout_height: 0.0,
            home_native_input_lines: None,
            xhs_cookie_focus: cx.focus_handle(),
            xhs_cookie_scroll: ScrollHandle::new(),
            xhs_cookie_scrollbar_drag: widgets::ScrollbarDrag::default(),
            xhs_cookie_card_height: 0.0,
            xhs_cookie_card_top: None,
            xhs_cookie_ime_follow: false,
            #[cfg(target_os = "android")]
            xhs_cookie_ime_preview: false,
            #[cfg(target_os = "android")]
            xhs_cookie_warmup_frames: 0,
            #[cfg(target_os = "android")]
            xhs_cookie_editor_warmup: false,
            xhs_cookie_native_input_lines: None,
            results_scroll: ScrollHandle::new(),
            loading: false,
            spinner_phase: 0.0,
            results: Vec::new(),
            toast: None,
            toast_seq: 0,
            dialog: None,
            cache_size: 0,
            cache_paths: Vec::new(),
            download: None,
            download_rx: None,
            location_dialog_directory: String::new(),
            location_dialog_remember: false,
            live_photo_dialog_choice: false,
            xhs_cookie_input: String::new(),
            _appearance_sub,
            #[cfg(target_os = "android")]
            android_back_pressed_at: None,
        };
        view.refresh_cache_info();
        view
    }

    pub fn palette(&self) -> Palette {
        Palette::new(self.dark())
    }

    pub fn dark(&self) -> bool {
        self.settings.is_dark(self.system_dark)
    }

    pub fn save_settings(&self) {
        let _ = self.settings.save(&self.paths.settings_file());
    }

    pub fn refresh_cache_info(&mut self) {
        let mut cache_dirs = vec![self.paths.cache_dir.clone()];
        cache_dirs.extend(self.paths.legacy_cache_dirs());
        self.cache_size = 0;
        self.cache_paths.clear();
        for dir in cache_dirs {
            let size = if dir.is_dir() {
                crate::paths::dir_size(&dir)
            } else {
                crate::paths::allocated_file_size(&dir)
            };
            if size > 0 {
                self.cache_size += size;
                self.cache_paths
                    .push(format!("下载缓存目录: {}", dir.display()));
            }
        }
        let leftovers = crate::paths::leftover_download_files();
        if !leftovers.is_empty() {
            self.cache_size += leftovers
                .iter()
                .map(|file| crate::paths::allocated_file_size(file))
                .sum::<u64>();
            self.cache_paths
                .push(format!("文档目录残留: {} 个旧下载文件", leftovers.len()));
        }
    }

    pub fn toast(&mut self, message: impl Into<String>, kind: ToastKind, cx: &mut Context<Self>) {
        self.toast_seq += 1;
        let seq = self.toast_seq;
        self.toast = Some(Toast {
            message: message.into(),
            kind,
            seq,
        });
        cx.notify();
        let duration = Duration::from_millis(1800);
        cx.spawn(async move |this: WeakEntity<AppView>, cx| {
            cx.background_executor().timer(duration).await;
            let _ = this.update(cx, |this, cx| {
                if this.toast.as_ref().map(|toast| toast.seq) == Some(seq) {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// 打开一个二级页面，新页面从右侧滑入。
    pub fn navigate_to(&mut self, to: Route, cx: &mut Context<Self>) {
        self.settle_transition_now();
        hide_soft_keyboard();
        let from = self.route;
        if from == to {
            cx.notify();
            return;
        }

        self.route = to;
        self.transition = Some(PageTransition {
            from,
            to,
            kind: PageTransitionKind::Push,
            progress: 0.0,
            dragging: false,
            animation: Some(PageAnimation {
                started_at: Instant::now(),
                from_progress: 0.0,
                to_progress: 1.0,
            }),
        });
        self.start_transition_ticker(cx);
        cx.notify();
    }

    /// 返回首页，当前页面整体向右滑出。
    pub fn navigate_back(&mut self, cx: &mut Context<Self>) {
        if self.transition.is_some() || self.route == Route::Home {
            return;
        }

        hide_soft_keyboard();
        let from = self.route;
        self.route = Route::Home;
        self.transition = Some(PageTransition {
            from,
            to: Route::Home,
            kind: PageTransitionKind::Pop,
            progress: 0.0,
            dragging: false,
            animation: Some(PageAnimation {
                started_at: Instant::now(),
                from_progress: 0.0,
                to_progress: 1.0,
            }),
        });
        self.start_transition_ticker(cx);
        cx.notify();
    }

    pub fn begin_edge_back_drag(&mut self, cx: &mut Context<Self>) {
        if self.transition.is_some() || self.route == Route::Home {
            return;
        }

        hide_soft_keyboard();
        let from = self.route;
        self.route = Route::Home;
        self.transition = Some(PageTransition {
            from,
            to: Route::Home,
            kind: PageTransitionKind::Pop,
            progress: 0.0,
            dragging: true,
            animation: None,
        });
        cx.notify();
    }

    pub fn update_edge_back_drag(&mut self, progress: f32, cx: &mut Context<Self>) {
        let Some(transition) = self.transition.as_mut() else {
            return;
        };
        if !transition.dragging {
            return;
        }
        transition.progress = progress.clamp(0.0, 1.0);
        cx.notify();
    }

    pub fn end_edge_back_drag(&mut self, cx: &mut Context<Self>) {
        let Some(transition) = self.transition.as_mut() else {
            return;
        };
        if !transition.dragging {
            return;
        }

        let target = if transition.progress >= EDGE_BACK_THRESHOLD {
            1.0
        } else {
            0.0
        };
        transition.dragging = false;
        transition.animation = Some(PageAnimation {
            started_at: Instant::now(),
            from_progress: transition.progress,
            to_progress: target,
        });
        self.start_transition_ticker(cx);
        cx.notify();
    }

    /// Android 返回键 / Escape：优先关闭弹窗，其次退出二级页面。
    pub fn handle_page_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        hide_soft_keyboard();
        if self.dialog.is_some() {
            crate::dialogs::close_or_replace_dialog(self, window, cx);
            return;
        }
        if self.transition.is_some() {
            return;
        }
        if self.route != Route::Home {
            self.navigate_back(cx);
        } else if window.is_fullscreen() {
            window.toggle_fullscreen();
        }
    }

    #[cfg(target_os = "android")]
    fn handle_android_system_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let at_home =
            self.dialog.is_none() && self.transition.is_none() && self.route == Route::Home;
        if !at_home {
            self.android_back_pressed_at = None;
            self.handle_page_back(window, cx);
            return;
        }

        let now = Instant::now();
        let should_exit_app = self
            .android_back_pressed_at
            .is_some_and(|pressed_at| now.duration_since(pressed_at) <= Duration::from_secs(2));
        if should_exit_app {
            self.android_back_pressed_at = None;
            hide_soft_keyboard();
            gpui_android::exit_app();
            return;
        }

        self.android_back_pressed_at = Some(now);
        hide_soft_keyboard();
        self.toast("再按一次退出", ToastKind::Info, cx);
    }

    fn settle_transition_now(&mut self) {
        let Some(transition) = self.transition.take() else {
            return;
        };
        let target = transition
            .animation
            .as_ref()
            .map(|animation| animation.to_progress)
            .unwrap_or(transition.progress);
        self.route = if target >= 0.5 {
            transition.to
        } else {
            transition.from
        };
    }

    fn start_transition_ticker(&mut self, cx: &mut Context<Self>) {
        self.transition_seq = self.transition_seq.wrapping_add(1);
        let seq = self.transition_seq;
        cx.spawn(async move |this: WeakEntity<AppView>, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let outcome = this.update(cx, |this, cx| {
                if this.transition_seq != seq {
                    return true;
                }
                let running = this.advance_transition();
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

    /// 推进页面动画；返回 true 表示还需要下一帧。
    fn advance_transition(&mut self) -> bool {
        let Some(transition) = self.transition.as_mut() else {
            return false;
        };
        if transition.dragging {
            return false;
        }
        let Some(animation) = transition.animation.as_ref() else {
            return false;
        };

        let elapsed = animation.started_at.elapsed().as_secs_f32();
        let linear = (elapsed / PAGE_TRANSITION_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - linear).powi(3);
        transition.progress = (animation.from_progress
            + (animation.to_progress - animation.from_progress) * eased)
            .clamp(0.0, 1.0);

        if linear < 1.0 {
            return true;
        }

        let from = transition.from;
        let to = transition.to;
        let target = animation.to_progress;
        self.route = if target >= 0.5 { to } else { from };
        self.transition = None;
        if self.route != Route::Home && self.dialog.is_none() {
            #[cfg(target_os = "android")]
            dismiss_native_editor_now();
            #[cfg(not(target_os = "android"))]
            dismiss_native_editor();
        }
        false
    }
}

impl AppView {
    #[cfg(target_os = "ios")]
    fn apply_ios_native_input_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = ios_native_input::drain_events();
        for event in events.text {
            if event.id == "url-input" && self.route == Route::Home && self.dialog.is_none() {
                self.input = event.text;
                #[cfg(target_os = "ios")]
                ios_native_input::confirm_native_text("url-input", &self.input);
                cx.notify();
            } else if event.id == dialogs::XHS_COOKIE_INPUT_ID
                && matches!(self.dialog, Some(Dialog::XhsCookie))
            {
                self.xhs_cookie_input = event.text;
                #[cfg(target_os = "ios")]
                ios_native_input::confirm_native_text(
                    dialogs::XHS_COOKIE_INPUT_ID,
                    &self.xhs_cookie_input,
                );
                cx.notify();
            }
        }
        for event in events.height {
            let line_height = douyin_input::MULTILINE_LINE_HEIGHT.max(1.0);
            let rows = ((event.height - ios_native_input::TEXT_PADDING * 2.0) / line_height)
                .round()
                .max(1.0) as usize;
            if event.id == "url-input" && self.route == Route::Home && self.dialog.is_none() {
                if event.text == self.input {
                    self.home_native_input_lines = Some(rows);
                    cx.notify();
                }
            } else if event.id == dialogs::XHS_COOKIE_INPUT_ID
                && matches!(self.dialog, Some(Dialog::XhsCookie))
            {
                if event.text == self.xhs_cookie_input {
                    self.xhs_cookie_native_input_lines = Some(rows);
                    cx.notify();
                }
            }
        }
        for event in events.focus {
            match event.id.as_str() {
                "url-input" => {
                    if event.focused
                        && self.route == Route::Home
                        && self.dialog.is_none()
                        && !self.input_focus.is_focused(window)
                    {
                        window.focus(&self.input_focus, cx);
                    } else if !event.focused && self.input_focus.is_focused(window) {
                        window.blur(cx);
                    }
                    if !event.focused {
                        self.home_native_input_lines = None;
                    }
                }
                dialogs::XHS_COOKIE_INPUT_ID => {
                    if event.focused
                        && matches!(self.dialog, Some(Dialog::XhsCookie))
                        && !self.xhs_cookie_focus.is_focused(window)
                    {
                        window.focus(&self.xhs_cookie_focus, cx);
                    } else if !event.focused && self.xhs_cookie_focus.is_focused(window) {
                        window.blur(cx);
                    }
                    if !event.focused {
                        self.xhs_cookie_native_input_lines = None;
                    }
                }
                _ => {}
            }
        }
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(target_os = "ios")]
        self.apply_ios_native_input_events(window, cx);
        sync_system_chrome(self.dark());
        let palette = self.palette();
        let input_bounds = self.input_scroll.bounds();
        let home_input_active = self.route == Route::Home && self.dialog.is_none();
        #[cfg(target_os = "android")]
        let viewport_width = f32::from(window.fully_visible_bounds().size.width);
        #[cfg(not(target_os = "android"))]
        let viewport_width = f32::from(window.bounds().size.width);
        let transition = self.transition.as_ref().map(|transition| {
            (
                transition.from,
                transition.to,
                transition.kind,
                transition.progress,
            )
        });
        let toast = self.render_toast(palette);
        #[cfg(target_os = "android")]
        self.sync_cookie_dialog_native_offset(window);
        let dialog = self.render_dialog(palette, window, cx);

        let root = div()
            .relative()
            .size_full()
            .bg(palette.bg)
            .text_color(palette.text)
            .font_family(theme::ui_font())
            .text_size(px(14.0))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, move |event, _window, _cx| {
                if home_input_active {
                    if input_bounds.contains(&event.position) {
                        #[cfg(not(any(target_os = "android", target_os = "ios")))]
                        show_soft_keyboard();
                    } else {
                        #[cfg(not(any(target_os = "android", target_os = "ios")))]
                        {
                            _window.blur(_cx);
                            hide_soft_keyboard();
                        }
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, move |_event, _window, _cx| {
                #[cfg(target_os = "ios")]
                if home_input_active && !input_bounds.contains(&_event.position) {
                    // Complete the sibling control's click before dismissing
                    // the native first responder. Blurring during MouseDown
                    // removes the pressed control on UIKit and requires a
                    // second tap for Parse/Settings/Clear.
                    _window.defer(_cx, |window, cx| {
                        window.blur(cx);
                        hide_soft_keyboard();
                    });
                }
                #[cfg(target_os = "android")]
                if home_input_active && !input_bounds.contains(&_event.position) {
                    // Blur only after the complete gesture so the pressed
                    // native/mobile sibling control receives its click callback.
                    _window.defer(_cx, |window, cx| {
                        window.blur(cx);
                        hide_soft_keyboard();
                    });
                }
            });

        let mut stage = div()
            .relative()
            .flex_1()
            .w_full()
            .min_h(px(0.0))
            .overflow_hidden();

        if let Some((from, to, kind, progress)) = transition {
            let (base_route, moving_route) = match kind {
                PageTransitionKind::Push => (from, to),
                PageTransitionKind::Pop => (to, from),
            };
            let base = self.render_page(base_route, window, cx);
            let moving = self.render_page(moving_route, window, cx);

            // 每个页面各自独占一个全屏图层，图层按 inset_0 填充父容器，
            // 页面用 w_full 填满图层：布局宽度完全由实际容器决定，
            // 不使用任何测量的"可见区"像素宽度（刘海/系统栏/键盘动画期间
            // 这些值会瞬变，横屏时会把整页压窄再弹回）。
            // 滑动页和 native editor clip 共用同一个屏幕宽度偏移量，
            // 逐帧绝对同步，避免两边各自推算几何产生滞后或误差。
            let slide_offset =
                transition_slide_offset(f32::from(window.viewport_size().width), kind, progress);
            stage = stage
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .overflow_hidden()
                        .bg(palette.bg)
                        .child(base),
                )
                .child(
                    div().absolute().inset_0().overflow_hidden().child(
                        div()
                            .absolute()
                            .top(px(0.0))
                            .left(px(slide_offset))
                            .w_full()
                            .h_full()
                            .flex_shrink_0()
                            .bg(palette.bg)
                            .child(moving),
                    ),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex_shrink_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        }),
                );
        } else {
            stage = stage.child(self.render_page(self.route, window, cx));
        }

        if cfg!(target_os = "ios") {
            let edge_back_enabled = self.transition.is_some() || self.route != Route::Home;
            stage = stage.child(self.render_edge_back_strip(viewport_width, edge_back_enabled, cx));
        }

        let root = root.child(stage).children(toast).children(dialog);

        #[cfg(target_os = "ios")]
        let root = {
            let (top, left, bottom, right) = gpui_ios::safe_area_insets();
            root.pt(px(top)).pb(px(bottom)).pl(px(left)).pr(px(right))
        };
        #[cfg(target_os = "android")]
        let root = {
            // 只按系统栏/刘海的稳定 safe area 内边距，键盘相关空间由
            // 窗口自身 resize 与输入框高度逻辑处理；过渡/键盘动画期间
            // fully_visible_bounds 会瞬变，不能参与布局，否则整页宽度抖动。
            let (left, top, right, bottom) = gpui_android::safe_area_insets_logical();
            root.pt(px(top)).pb(px(bottom)).pl(px(left)).pr(px(right))
        };

        root
    }
}

impl AppView {
    fn render_page(
        &mut self,
        route: Route,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.palette();
        let titlebar = self.render_titlebar(route, window, cx);
        if !is_desktop() && route == Route::Home {
            let compact_input = self.compact_home_input_lines(window).is_some();
            let home_content = self.render_home_content(palette, window, cx);
            let mut content = div().w_full().flex().flex_col();
            if !compact_input {
                content = content.child(titlebar);
            }
            content = content.child(home_content);
            return div()
                .size_full()
                .bg(palette.bg)
                .child(
                    div()
                        .id("home-scroll")
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.home_scroll)
                        .child(content),
                )
                .into_any_element();
        }

        let content = match route {
            Route::Home => self.render_home(palette, window, cx),
            Route::Results => self.render_results_page(palette, window, cx),
            Route::Settings => self.render_settings(palette, window, cx),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.bg)
            .child(titlebar)
            .child(content)
            .into_any_element()
    }

    fn render_edge_back_strip(
        &self,
        viewport_width: f32,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let strip = div()
            .id("edge-back-strip")
            .absolute()
            .left(px(0.0))
            // The titlebar owns the back button. Starting below it avoids a
            // short tap on the button being captured as an edge-back drag.
            .top(px(MOBILE_APPBAR_HEIGHT))
            .w(px(24.0))
            .bottom(px(0.0));

        if !enabled {
            return strip.into_any_element();
        }

        let down_entity = cx.entity();
        let move_entity = down_entity.clone();
        let up_entity = down_entity.clone();
        let cancel_entity = down_entity.clone();
        let width = viewport_width.max(1.0);

        strip
            .occlude()
            .on_pointer_down(move |event, window, cx| {
                if window.capture_direct_pointer(event.pointer_id) {
                    down_entity.update(cx, |this, cx| this.begin_edge_back_drag(cx));
                }
            })
            .on_pointer_move(move |event, _, cx| {
                let progress = f32::from(event.position.x) / width;
                move_entity.update(cx, |this, cx| {
                    this.update_edge_back_drag(progress, cx);
                });
            })
            .on_pointer_up(move |_, _, cx| {
                up_entity.update(cx, |this, cx| this.end_edge_back_drag(cx));
            })
            .on_pointer_cancel(move |_, _, cx| {
                cancel_entity.update(cx, |this, cx| this.end_edge_back_drag(cx));
            })
            .into_any_element()
    }

    fn render_toast(&self, palette: Palette) -> Option<AnyElement> {
        let toast = self.toast.as_ref()?;
        let bg = palette.toast_bg;
        let border = if palette.dark {
            gpui::rgb(0x2a3444)
        } else {
            gpui::rgb(0xebeef5)
        };
        let fg = if palette.dark {
            gpui::rgb(0xffffff)
        } else {
            gpui::rgb(0x000000)
        };
        Some(
            div()
                .absolute()
                .left(px(16.0))
                .right(px(16.0))
                .bottom(px(24.0))
                .flex()
                .justify_center()
                .child(
                    div()
                        .min_w(px(0.0))
                        .max_w(px(560.0))
                        .px(px(16.0))
                        .py(px(10.0))
                        .rounded(px(8.0))
                        .bg(bg)
                        .border_1()
                        .border_color(border)
                        .text_color(fg)
                        .text_size(px(15.0))
                        .flex()
                        .items_center()
                        .child(toast.message.clone()),
                )
                .into_any_element(),
        )
    }
}

/// 打开主窗口（桌面端 1024x720 居中、无标题栏；移动端全屏）。
pub fn open_main_window(cx: &mut App) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    if cx.try_global::<MacWindowClosedGuard>().is_none() {
        douyin_input::clear_all_input_states();
    }
    #[cfg(target_os = "macos")]
    let window_closed = cx
        .try_global::<MacWindowClosedGuard>()
        .map(|_| None)
        .unwrap_or_else(|| {
            Some(cx.on_window_closed(|_, _| douyin_input::clear_all_input_states()))
        });

    let paths = paths::resolve();
    let settings = AppSettings::load(&paths.settings_file());
    let desktop = is_desktop();

    let bounds = Bounds::centered(None, size(px(1024.0), px(720.0)), cx);
    let titlebar = TitlebarOptions {
        title: Some(SharedString::from(APP_NAME)),
        appears_transparent: desktop,
        traffic_light_position: None,
    };

    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    let window = cx.open_window(
        WindowOptions {
            window_bounds: if desktop {
                Some(WindowBounds::Windowed(bounds))
            } else {
                None
            },
            window_min_size: if desktop {
                Some(size(px(460.0), px(600.0)))
            } else {
                None
            },
            titlebar: Some(titlebar),
            app_owns_titlebar_drag: false,
            kind: WindowKind::Normal,
            is_resizable: true,
            is_minimizable: true,
            show: true,
            focus: true,
            ..Default::default()
        },
        move |window, cx| cx.new(|cx| AppView::new(paths.clone(), settings.clone(), window, cx)),
    )?;
    #[cfg(target_os = "macos")]
    if let Some(window_closed) = window_closed {
        window.update(cx, |_, _, cx| {
            cx.set_global(MacWindowClosedGuard {
                _window_closed: window_closed,
            })
        })?;
    }

    Ok(())
}

/// 注册全局主题（读取本地设置）。
pub fn init(cx: &mut App) {
    // The iOS backend invokes this callback synchronously from UIKit's
    // keyboard notifications. GPUI already owns the App while dispatching the
    // underlying interaction, so the callback's `borrow_mut()` can panic.
    // Native text views own keyboard input here; the UI reads the updated
    // keyboard height directly from gpui_ios and repaints through text input
    // invalidation instead.
    #[cfg(target_os = "ios")]
    gpui_ios::set_keyboard_layout_change_callback(None);

    let settings = AppSettings::load(&paths::resolve().settings_file());
    let dark = match settings.theme_mode.as_str() {
        "light" => false,
        "dark" => true,
        _ => system_theme_dark(),
    };
    apply_theme(cx, dark);

    install_menus(cx);

    // Mobile only: hand focused inputs over to the platform's real editor
    // (system keyboard, caret, selection handles, and edit menu).
    #[cfg(any(target_os = "android", target_os = "ios"))]
    douyin_input::set_text_menu_hooks(
        Some(std::rc::Rc::new(
            |request: &douyin_input::TextMenuRequest| {
                #[cfg(target_os = "android")]
                gpui_android::show_native_editor(gpui_android::NativeEditorParams {
                    text: request.text.clone(),
                    selection_utf16: request.selection_utf16,
                    bounds: request.bounds,
                    clip_bounds: request.clip_bounds,
                    // Android only: the y value the native editor receives is
                    // the first-line baseline, not the line box top, so the
                    // `EditText` can align its glyphs to GPUI's exact row.
                    text_origin: (request.text_origin.0, request.text_baseline),
                    font_size: request.font_size,
                    multiline: request.multiline,
                    wrap_width: request.wrap_width,
                    line_height: request.line_height,
                    line_ranges_utf16: request.line_ranges_utf16.clone(),
                    initial: request.initial,
                    force_text_sync: request.force_text_sync,
                    text_color: request.text_color,
                    background_color: request.background_color,
                    border_color: request.border_color,
                    focus_border_color: request.focus_border_color,
                    border_width: request.border_width,
                    placeholder: request.placeholder.clone(),
                    placeholder_color: request.placeholder_color,
                    corner_radius: request.corner_radius,
                });
                #[cfg(target_os = "ios")]
                ios_native_input::show(request);
            },
        )),
        Some(std::rc::Rc::new(|| {
            #[cfg(target_os = "android")]
            {
                gpui_android::hide_native_editor();
                gpui_android::hide_keyboard();
            }
            #[cfg(target_os = "ios")]
            {
                ios_native_input::hide_active();
            }
        })),
        #[cfg(target_os = "ios")]
        Some(std::rc::Rc::new(|| ios_native_input::current_text())),
        #[cfg(target_os = "android")]
        Some(std::rc::Rc::new(|| gpui_android::native_editor_text())),
    );
    #[cfg(target_os = "ios")]
    douyin_input::set_text_menu_hide_field_hook(Some(std::rc::Rc::new(|id| {
        ios_native_input::hide(id);
    })));
    #[cfg(target_os = "android")]
    gpui_android::set_native_editor_scroll_callback(Some(Box::new(
        douyin_input::apply_native_editor_scroll,
    )));
    #[cfg(target_os = "android")]
    gpui_android::set_native_editor_hidden_callback(Some(Box::new(|| {
        douyin_input::mark_native_editor_hidden();
    })));
    #[cfg(target_os = "android")]
    gpui_android::set_native_editor_focus_callback(Some(Box::new(
        douyin_input::handle_native_editor_focus,
    )));
    #[cfg(target_os = "android")]
    douyin_input::set_text_menu_blur_hook(Some(std::rc::Rc::new(|| {
        gpui_android::blur_native_editor();
    })));
    #[cfg(target_os = "android")]
    douyin_input::set_native_editor_line_ranges_hook(Some(std::rc::Rc::new(|| {
        gpui_android::native_editor_line_ranges().map(|ranges| (ranges.text, ranges.ranges_utf16))
    })));

    cx.bind_keys([gpui::KeyBinding::new("escape", PageBack, None)]);
    cx.on_action(|_: &PageBack, cx: &mut App| {
        cx.defer(|cx| {
            let Some(handle) = cx.active_window() else {
                return;
            };
            let Some(handle) = handle.downcast::<AppView>() else {
                return;
            };
            let _ = handle.update(cx, |view, window, cx| {
                view.handle_page_back(window, cx);
            });
        });
    });
    cx.on_action(|_: &ExitFullscreen, cx: &mut App| {
        cx.defer(|cx| {
            let Some(handle) = cx.active_window() else {
                return;
            };
            let _ = handle.update(cx, |_, window, _| {
                if window.is_fullscreen() {
                    window.toggle_fullscreen();
                }
            });
        });
    });
    cx.on_action(|_: &ToggleFullscreen, cx: &mut App| {
        cx.defer(|cx| {
            let Some(handle) = cx.active_window() else {
                return;
            };
            let _ = handle.update(cx, |_, window, _| window.toggle_fullscreen());
        });
    });
    cx.on_action(|_: &About, cx: &mut App| {
        cx.defer(|cx| {
            let Some(handle) = cx.active_window() else {
                return;
            };
            let Some(handle) = handle.downcast::<AppView>() else {
                return;
            };
            let _ = handle.update(cx, |view, _, cx| {
                view.dialog = Some(Dialog::About);
                cx.notify();
            });
        });
    });
    cx.on_action(|_: &Help, cx: &mut App| {
        cx.defer(|cx| {
            let Some(handle) = cx.active_window() else {
                return;
            };
            let Some(handle) = handle.downcast::<AppView>() else {
                return;
            };
            let _ = handle.update(cx, |view, _, cx| {
                view.dialog = Some(Dialog::Help);
                cx.notify();
            });
        });
    });
    cx.on_action(|_: &Minimize, cx: &mut App| {
        cx.defer(|cx| {
            let Some(handle) = cx.active_window() else {
                return;
            };
            let _ = handle.update(cx, |_, window, _| window.minimize_window());
        });
    });
    cx.on_action(|_: &Zoom, cx: &mut App| {
        cx.defer(|cx| {
            let Some(handle) = cx.active_window() else {
                return;
            };
            let _ = handle.update(cx, |_, window, _| window.zoom_window());
        });
    });
    cx.on_action(|_: &Quit, cx: &mut App| {
        cx.quit();
    });
}

/// Resolve the system half of `theme_mode: "system"` before a window exists.
fn system_theme_dark() -> bool {
    #[cfg(target_os = "android")]
    {
        gpui_android::android::jni::query_night_mode_via_jni()
    }
    #[cfg(not(target_os = "android"))]
    {
        false
    }
}

fn install_menus(cx: &mut App) {
    cx.set_menus([
        Menu::new(APP_NAME).items([
            MenuItem::action("关于短视频解析", About),
            MenuItem::separator(),
            MenuItem::action("退出短视频解析", Quit),
        ]),
        Menu::new("编辑").items([
            MenuItem::os_action("撤销", gpui::NoAction, OsAction::Undo),
            MenuItem::os_action("重做", gpui::NoAction, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("剪切", gpui::NoAction, OsAction::Cut),
            MenuItem::os_action("复制", gpui::NoAction, OsAction::Copy),
            MenuItem::os_action("粘贴", gpui::NoAction, OsAction::Paste),
            MenuItem::os_action("全选", gpui::NoAction, OsAction::SelectAll),
        ]),
        Menu::new("显示").items([MenuItem::action("切换全屏", ToggleFullscreen)]),
        Menu::new("窗口").items([
            MenuItem::action("最小化", Minimize),
            MenuItem::action("缩放", Zoom),
        ]),
        Menu::new("帮助").items([MenuItem::action("使用帮助", Help)]),
    ]);
}

/// 将 GPUI 主题同步为应用配色（输入框光标 / 选区颜色取自 accent）。
pub fn apply_theme(cx: &mut App, dark: bool) {
    use std::sync::Arc;

    let palette = Palette::new(dark);
    let variant = if dark {
        ThemeVariant::Dark
    } else {
        ThemeVariant::Light
    };
    let mut theme = gpui_ui_kit::theme::Theme::for_variant(variant);
    theme.font_family = theme::ui_font().into();
    theme.background = palette.bg;
    theme.surface = palette.bg;
    theme.text_primary = palette.text;
    theme.text_secondary = palette.text_secondary;
    theme.text_muted = palette.text_muted;
    theme.accent = palette.primary;
    theme.accent_hover = gpui::rgb(0x4f97fa);
    theme.border = palette.input_border;
    theme.border_hover = palette.primary;
    cx.set_global(ThemeState {
        theme: Arc::new(theme),
        follow_system: false,
    });
}

#[cfg(target_os = "ios")]
pub fn refresh_system_appearance(cx: &mut App, system_dark: bool) {
    let handles = cx
        .windows()
        .into_iter()
        .filter_map(|handle| handle.downcast::<AppView>())
        .collect::<Vec<_>>();
    for handle in &handles {
        let _ = handle.update(cx, |view, _, cx| {
            view.system_dark = system_dark;
            apply_theme(cx, view.dark());
            sync_system_chrome(view.dark());
            cx.notify();
        });
    }
    if !handles.is_empty() {
        cx.refresh_windows();
    } else {
        apply_theme(cx, system_dark);
        sync_system_chrome(system_dark);
        cx.refresh_windows();
    }
}

/// Sync mobile system-bar text/icons with the effective application theme.
fn sync_system_chrome(dark: bool) {
    #[cfg(target_os = "android")]
    {
        let background = if dark { 0x141414 } else { 0xffffff };
        gpui_android::set_system_chrome(&gpui_android::SystemChromeStyle {
            status_bar_color: Some(background),
            status_bar_style: if dark {
                gpui_android::StatusBarContentStyle::Light
            } else {
                gpui_android::StatusBarContentStyle::Dark
            },
            navigation_bar_color: Some(background),
        });
    }

    #[cfg(target_os = "ios")]
    {
        use std::sync::atomic::{AtomicI8, Ordering};

        static LAST_STYLE: AtomicI8 = AtomicI8::new(-1);
        let value = i8::from(dark);
        if LAST_STYLE.swap(value, Ordering::Relaxed) != value {
            gpui_ios::ios::set_status_bar_style(if dark {
                gpui_ios::StatusBarContentStyle::Light
            } else {
                gpui_ios::StatusBarContentStyle::Dark
            });
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = dark;
}

pub fn is_double_click(event: &gpui::ClickEvent) -> bool {
    match event {
        gpui::ClickEvent::Mouse(event) => event.up.click_count >= 2,
        gpui::ClickEvent::Touch(event) => event.tap_count >= 2,
        gpui::ClickEvent::Keyboard(_) => false,
    }
}

/// Request the mobile soft keyboard (no-op on desktop).
pub(crate) fn show_soft_keyboard() {
    #[cfg(target_os = "android")]
    gpui_android::show_keyboard();

    #[cfg(target_os = "ios")]
    {
        // The application-owned native UITextView becomes first responder
        // when its Input is focused; the hidden GPUI client must stay idle.
    }
}

/// Dismiss the mobile soft keyboard (no-op on desktop).
pub(crate) fn hide_soft_keyboard() {
    #[cfg(target_os = "android")]
    gpui_android::hide_keyboard();

    #[cfg(target_os = "ios")]
    ios_native_input::hide_active();
}

/// Collapse the mobile native editor takeover (no-op on desktop).
pub(crate) fn dismiss_native_editor() {
    #[cfg(target_os = "android")]
    gpui_android::hide_native_editor();
    #[cfg(target_os = "ios")]
    ios_native_input::hide_all();
}

pub(crate) fn dismiss_native_editor_now() {
    #[cfg(target_os = "android")]
    gpui_android::hide_native_editor_now();
    #[cfg(target_os = "ios")]
    ios_native_input::hide_all();
}
