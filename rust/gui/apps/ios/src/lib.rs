//! iOS 静态库入口：由 Swift AppDelegate 调用 `douyin_parser_ios_start`。

use std::path::PathBuf;

use gpui::App;

fn install_paths() {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    douyin_ui::paths::set_override(douyin_ui::paths::AppPaths::with_roots(
        home.join(".config").join("douyin_parser"),
        home.join("Library").join("Caches").join("douyin_parser"),
        home.join("Documents"),
    ));
}

fn launch(cx: &mut App) {
    douyin_ui::init(cx);
    if let Err(error) = douyin_ui::open_main_window(cx) {
        log::error!("窗口创建失败: {error}");
        return;
    }
    cx.activate(true);
}

#[unsafe(no_mangle)]
pub extern "C" fn douyin_parser_ios_start() {
    let _ = oslog::OsLogger::new("com.dreamfly.douyinParser")
        .level_filter(log::LevelFilter::Info)
        .init();
    install_paths();
    gpui_ios::ios::ffi::set_app_callback(Box::new(launch));
    gpui_ios::ios::ffi::run_app();
}
