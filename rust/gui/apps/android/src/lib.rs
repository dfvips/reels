//! Android 动态库入口：由 `dev.gpui.mobile.GpuiActivity` 加载并调用 `android_main`。

#[cfg(target_os = "android")]
mod imp {
    use std::path::PathBuf;

    use gpui::{App, Application};

    fn install_paths(app: &android_activity::AndroidApp) {
        let base: PathBuf = app.internal_data_path().unwrap_or_else(std::env::temp_dir);
        let cache: PathBuf = app
            .external_data_path()
            .or_else(|| app.internal_data_path())
            .unwrap_or_else(std::env::temp_dir)
            .join("cache");
        douyin_ui::paths::set_override(douyin_ui::paths::AppPaths::with_roots(
            base.join("config"),
            cache,
            base.join("downloads"),
        ));
    }

    #[unsafe(no_mangle)]
    pub fn android_main(app: android_activity::AndroidApp) {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("douyin-parser"),
        );
        std::panic::set_hook(Box::new(|info| {
            log::error!("GPUI Android panic: {info}");
        }));

        install_paths(&app);
        let _platform = gpui_android::android::jni::init_platform(&app);
        let Some(shared_platform) = gpui_android::android::jni::shared_platform() else {
            log::error!("android_main: shared_platform() 返回空");
            return;
        };

        Application::with_platform(shared_platform.into_rc()).run(|cx: &mut App| {
            douyin_ui::init(cx);
            if let Err(error) = douyin_ui::open_main_window(cx) {
                log::error!("窗口创建失败: {error}");
                return;
            }
            cx.activate(true);
        });
    }
}
