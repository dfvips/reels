#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::{actions, App, Application, KeyBinding};

actions!(reels, [Quit]);

mod desktop_sound;
#[cfg(target_os = "macos")]
mod macos_dock_progress;
#[cfg(target_os = "macos")]
mod macos_icon;
#[cfg(target_os = "windows")]
mod windows_idm;
#[cfg(target_os = "windows")]
mod windows_text_menu;

fn main() {
    let platform = match gpui_miniapp::current_platform() {
        Ok(platform) => platform,
        Err(error) => {
            eprintln!("平台初始化失败: {error}");
            return;
        }
    };

    Application::with_platform(platform).run(|cx: &mut App| {
        douyin_ui::set_download_sound_hook(Some(Box::new(desktop_sound::play_finish_sound)));
        #[cfg(target_os = "windows")]
        douyin_ui::set_idm_export_hook(Some(Box::new(|links| {
            windows_idm::send_links(links).map_err(anyhow::Error::msg)
        })));
        #[cfg(target_os = "windows")]
        windows_text_menu::install();
        #[cfg(target_os = "macos")]
        macos_icon::set_app_icon_for_system_appearance();
        #[cfg(target_os = "macos")]
        douyin_ui::set_download_icon_progress_hook(Some(Box::new(|progress| {
            macos_dock_progress::set_progress(progress);
        })));
        #[cfg(target_os = "macos")]
        macos_dock_progress::set_progress(None);
        #[cfg(target_os = "macos")]
        macos_dock_progress::install_cleanup();

        douyin_ui::init(cx);
        cx.bind_keys([KeyBinding::new("secondary-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        if let Err(error) = douyin_ui::open_main_window(cx) {
            eprintln!("窗口创建失败: {error}");
            cx.quit();
            return;
        }
        #[cfg(target_os = "macos")]
        macos_icon::refresh_app_icon();
        cx.activate(true);
    });
}
