#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::{actions, App, Application, KeyBinding};

actions!(douyin_parser, [Quit]);

fn main() {
    let platform = match gpui_miniapp::current_platform() {
        Ok(platform) => platform,
        Err(error) => {
            eprintln!("平台初始化失败: {error}");
            return;
        }
    };

    Application::with_platform(platform).run(|cx: &mut App| {
        douyin_ui::init(cx);
        cx.bind_keys([KeyBinding::new("secondary-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        if let Err(error) = douyin_ui::open_main_window(cx) {
            eprintln!("窗口创建失败: {error}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
