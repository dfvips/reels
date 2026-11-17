use douyin_core::settings::AppSettings;
use douyin_ui::paths;
use gpui::{App, AsyncApp};
use std::cell::RefCell;

thread_local! {
    static ASYNC_APP: RefCell<Option<AsyncApp>> = const { RefCell::new(None) };
}

pub(super) fn register_app(cx: &mut App) {
    ASYNC_APP.with(|slot| *slot.borrow_mut() = Some(cx.to_async()));
}

#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_refresh_system_appearance(system_dark: bool) {
    let settings = AppSettings::load(&paths::resolve().settings_file());
    if settings.theme_mode == "light" || settings.theme_mode == "dark" {
        return;
    }
    ASYNC_APP.with(|slot| {
        let Some(async_app) = slot.borrow().clone() else {
            return;
        };
        async_app.update(|cx| douyin_ui::refresh_system_appearance(cx, system_dark));
    });
    gpui_ios::ios::ffi::gpui_ios_request_current_frame();
}

/// App 当前实际展示的主题是否为深色（显式主题优先，否则跟随系统）。
#[unsafe(no_mangle)]
pub extern "C" fn reels_ios_app_theme_is_dark(system_dark: bool) -> bool {
    let settings = AppSettings::load(&paths::resolve().settings_file());
    match settings.theme_mode.as_str() {
        "light" => false,
        "dark" => true,
        _ => system_dark,
    }
}
