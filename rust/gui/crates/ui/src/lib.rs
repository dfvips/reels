//! 抖音视频解析 GPUI 界面。

pub mod dialogs;
pub mod download;
pub mod download_flow;
pub mod home;
pub mod icons;
pub mod paths;
pub mod settings_screen;
pub mod theme;
pub mod titlebar;
pub mod widgets;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use douyin_core::extractor::{MediaEntry, MediaKind, MediaPlatform};
use douyin_core::settings::AppSettings;
use gpui::{
    div, px, size, AnyElement, App, AppContext, Bounds, Context, FocusHandle, InteractiveElement,
    IntoElement, Menu, MenuItem, MouseButton, OsAction, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Subscription, TitlebarOptions, WeakEntity, Window, WindowAppearance,
    WindowBounds, WindowKind, WindowOptions,
};
use gpui_ui_kit::theme::{ThemeState, ThemeVariant};

use crate::download::DownloadEvent;
use crate::paths::{is_desktop, AppPaths};
use crate::theme::Palette;

pub const APP_NAME: &str = "抖音视频解析";
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
        Quit
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

    /// 主条目（RAW/最佳画质优先）。
    pub fn primary_entry(&self) -> Option<&MediaEntry> {
        self.entries.first()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Home,
    Results,
    Settings,
}

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

#[derive(Clone)]
pub enum Dialog {
    Help,
    About,
    SaveLocation,
    XhsCookie,
    ClearCache,
    ClearCacheResult {
        success: bool,
        message: String,
        details: Vec<String>,
    },
    DownloadConfirm,
    DownloadProgress,
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
    pub input: String,
    pub input_focus: FocusHandle,
    pub input_scroll: ScrollHandle,
    pub input_scroll_metrics: (f32, f32),
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
    pub xhs_cookie_input: String,
    _appearance_sub: Option<Subscription>,
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
                    cx.notify();
                }
            });
        }));
        let input_focus = cx.focus_handle();
        let mut view = Self {
            paths,
            settings,
            system_dark,
            route: Route::Home,
            input: String::new(),
            input_focus: input_focus.clone(),
            input_scroll: ScrollHandle::new(),
            input_scroll_metrics: (0.0, 0.0),
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
            xhs_cookie_input: String::new(),
            _appearance_sub,
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
        self.cache_size = crate::paths::dir_size(&self.paths.cache_dir);
        self.cache_paths = vec![format!("下载缓存目录: {}", self.paths.cache_dir.display())];
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
        cx.spawn(async move |this: WeakEntity<AppView>, cx| {
            cx.background_executor().timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |this, cx| {
                if this.toast.as_ref().map(|toast| toast.seq) == Some(seq) {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette();
        let input_bounds = self.input_scroll.bounds();
        let titlebar = self.render_titlebar(window, cx);
        let content = match self.route {
            Route::Home => self.render_home(palette, window, cx),
            Route::Results => self.render_results_page(palette, window, cx),
            Route::Settings => self.render_settings(palette, window, cx),
        };
        let toast = self.render_toast(palette);
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
            .child(titlebar)
            .child(content)
            .children(toast)
            .children(dialog)
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                if !input_bounds.contains(&event.position) {
                    window.blur(cx);
                }
            });

        #[cfg(target_os = "ios")]
        let root = {
            let (top, left, bottom, right) = gpui_ios::safe_area_insets();
            root.pt(px(top)).pb(px(bottom)).pl(px(left)).pr(px(right))
        };
        #[cfg(target_os = "android")]
        let root = root.pt(px(24.0)).pb(px(12.0));

        root
    }
}

impl AppView {
    fn render_toast(&self, palette: Palette) -> Option<AnyElement> {
        let toast = self.toast.as_ref()?;
        let bg = match toast.kind {
            ToastKind::Success => gpui::rgb(0x2e7d32),
            ToastKind::Error => gpui::rgba(0xc62828e6),
            ToastKind::Info => palette.toast_bg,
        };
        Some(
            div()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(24.0))
                .flex()
                .justify_center()
                .child(
                    div()
                        .max_w(px(560.0))
                        .px(px(16.0))
                        .py(px(10.0))
                        .rounded(px(8.0))
                        .bg(bg)
                        .text_color(gpui::rgb(0xffffff))
                        .text_size(px(15.0))
                        .child(toast.message.clone()),
                )
                .into_any_element(),
        )
    }
}

/// 打开主窗口（桌面端 1024x720 居中、无标题栏；移动端全屏）。
pub fn open_main_window(cx: &mut App) -> anyhow::Result<()> {
    let paths = paths::resolve();
    let settings = AppSettings::load(&paths.settings_file());
    let desktop = is_desktop();

    let bounds = Bounds::centered(None, size(px(1024.0), px(720.0)), cx);
    let titlebar = TitlebarOptions {
        title: Some(SharedString::from(APP_NAME)),
        appears_transparent: desktop,
        traffic_light_position: None,
    };

    cx.open_window(
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

    Ok(())
}

/// 注册全局主题（读取本地设置）。
pub fn init(cx: &mut App) {
    let settings = AppSettings::load(&paths::resolve().settings_file());
    let dark = matches!(settings.theme_mode.as_str(), "dark");
    apply_theme(cx, dark);

    install_menus(cx);

    cx.bind_keys([gpui::KeyBinding::new("escape", ExitFullscreen, None)]);
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

fn install_menus(cx: &mut App) {
    cx.set_menus([
        Menu::new(APP_NAME).items([
            MenuItem::action("关于抖音视频解析", About),
            MenuItem::separator(),
            MenuItem::action("退出抖音视频解析", Quit),
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

pub fn is_double_click(event: &gpui::ClickEvent) -> bool {
    match event {
        gpui::ClickEvent::Mouse(event) => event.up.click_count >= 2,
        gpui::ClickEvent::Touch(event) => event.tap_count >= 2,
        gpui::ClickEvent::Keyboard(_) => false,
    }
}
