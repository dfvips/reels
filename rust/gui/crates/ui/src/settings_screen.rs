//! 设置页：存储、Cookie、使用说明、关于、清除缓存。

use gpui::{
    div, px, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};

use crate::paths::{format_size, is_desktop};
use crate::theme::Palette;
use crate::widgets;
use crate::{icons, AppView, Dialog, Route, ToastKind, APP_VERSION};

impl AppView {
    pub fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.refresh_cache_info();
        self.navigate_to(Route::Settings, cx);
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.navigate_back(cx);
    }

    pub fn theme_name(&self) -> &'static str {
        match self.settings.theme_mode.as_str() {
            "light" => "浅色模式",
            "dark" => "深色模式",
            _ => "跟随系统",
        }
    }

    pub fn theme_label(&self) -> &'static str {
        match self.settings.theme_mode.as_str() {
            "light" => "浅色",
            "dark" => "深色",
            _ => "跟随系统",
        }
    }

    pub fn theme_icon(&self) -> &'static [u8] {
        match self.settings.theme_mode.as_str() {
            "light" => icons::SUN.as_bytes(),
            "dark" => icons::MOON.as_bytes(),
            _ => icons::CONTRAST.as_bytes(),
        }
    }

    /// 跟随系统 → 浅色 → 深色 → 跟随系统。
    pub fn cycle_theme(&mut self, cx: &mut Context<Self>) {
        let next = match self.settings.theme_mode.as_str() {
            "light" => "dark",
            "dark" => "system",
            _ => "light",
        };
        self.settings.theme_mode = next.to_string();
        self.save_settings();
        crate::apply_theme(cx, self.dark());
        let message = format!("已切换为{}", self.theme_name());
        self.toast(message, ToastKind::Info, cx);
        cx.notify();
    }

    pub fn open_clear_cache(&mut self, cx: &mut Context<Self>) {
        self.refresh_cache_info();
        self.dialog = Some(if self.cache_size == 0 {
            Dialog::NoCache
        } else {
            Dialog::ClearCache
        });
        cx.notify();
    }

    pub fn clear_cache(&mut self, cx: &mut Context<Self>) {
        let mut cache_dirs = vec![self.paths.cache_dir.clone()];
        cache_dirs.extend(self.paths.legacy_cache_dirs());
        cache_dirs.extend(crate::paths::leftover_download_files());
        cache_dirs.sort();
        cache_dirs.dedup();
        let mut errors = Vec::new();
        for path in cache_dirs {
            if !path.exists() {
                continue;
            }
            let result = if path.is_dir() {
                crate::download::clear_cache(&path)
            } else {
                std::fs::remove_file(&path).map_err(|error| error.to_string())
            };
            if let Err(error) = result {
                errors.push(format!("{}: {error}", path.display()));
            }
        }
        self.refresh_cache_info();
        let (success, message, details) = if errors.is_empty() && self.cache_size == 0 {
            (
                true,
                "缓存已清理".to_string(),
                vec![format!("当前剩余缓存: {}", format_size(self.cache_size))],
            )
        } else if !errors.is_empty() {
            errors.push(format!("当前剩余缓存: {}", format_size(self.cache_size)));
            (false, "清除缓存失败:".to_string(), errors)
        } else {
            (
                false,
                "仍有缓存未能清理:".to_string(),
                self.cache_paths
                    .iter()
                    .map(|path| format!("• {path}"))
                    .collect(),
            )
        };
        self.dialog = Some(Dialog::ClearCacheResult {
            success,
            message,
            details,
        });
        cx.notify();
    }

    pub fn render_settings(
        &mut self,
        palette: Palette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cache_size = format_size(self.cache_size);
        let cache_paths = self.cache_paths.len();
        let mut items: Vec<AnyElement> = Vec::new();

        if is_desktop() {
            let entity = cx.entity();
            items.push(
                self.render_item(
                    palette,
                    "settings-storage",
                    widgets::gradient_icon(icons::FOLDER.as_bytes(), 24.0, &cx.svg_renderer()),
                    "存储设置",
                    "设置视频保存的默认位置",
                    move |window, cx| {
                        window.blur(cx);
                        entity.update(cx, |this, cx| {
                            this.open_save_location_dialog(cx);
                        });
                    },
                )
                .into_any_element(),
            );
            items.push(widgets::divider(palette).into_any_element());
        }

        #[cfg(target_os = "ios")]
        {
            let entity = cx.entity();
            let live_subtitle = if self.settings.live_photo_export {
                "导出到相册时封装为实况照片"
            } else {
                "导出到相册时保存为图片和视频"
            };
            items.push(
                self.render_item(
                    palette,
                    "settings-live-photo",
                    widgets::gradient_icon(icons::LIVE_PHOTO, 24.0, &cx.svg_renderer()),
                    "实况照片",
                    live_subtitle,
                    move |window, cx| {
                        window.blur(cx);
                        entity.update(cx, |this, cx| {
                            this.live_photo_dialog_choice = this.settings.live_photo_export;
                            this.dialog = Some(Dialog::LivePhotoExport);
                            cx.notify();
                        });
                    },
                )
                .into_any_element(),
            );
            items.push(widgets::divider(palette).into_any_element());
        }

        let entity = cx.entity();
        let cookie_status = if self.settings.xhs_cookie.trim().is_empty() {
            "未设置"
        } else {
            "已设置（用于小红书解析）"
        };
        items.push(
            self.render_item(
                palette,
                "settings-cookie",
                widgets::gradient_icon(icons::LOCK.as_bytes(), 24.0, &cx.svg_renderer()),
                "Cookie 设置",
                &format!("仅用于小红书 · {cookie_status}"),
                move |window, cx| {
                    window.blur(cx);
                    entity.update(cx, |this, cx| {
                        this.open_xhs_cookie_dialog(cx);
                    });
                },
            )
            .into_any_element(),
        );
        items.push(widgets::divider(palette).into_any_element());

        let entity = cx.entity();
        items.push(
            self.render_item(
                palette,
                "settings-help",
                widgets::gradient_icon(icons::HELP.as_bytes(), 24.0, &cx.svg_renderer()),
                "使用说明",
                "查看支持的链接格式与使用方式",
                move |window, cx| {
                    window.blur(cx);
                    entity.update(cx, |this, cx| {
                        this.dialog = Some(Dialog::Help);
                        cx.notify();
                    });
                },
            )
            .into_any_element(),
        );
        items.push(widgets::divider(palette).into_any_element());

        let entity = cx.entity();
        items.push(
            self.render_item(
                palette,
                "settings-about",
                widgets::gradient_icon(icons::INFO.as_bytes(), 24.0, &cx.svg_renderer()),
                "关于",
                &format!("应用版本: {APP_VERSION}"),
                move |window, cx| {
                    window.blur(cx);
                    entity.update(cx, |this, cx| {
                        this.dialog = Some(Dialog::About);
                        cx.notify();
                    });
                },
            )
            .into_any_element(),
        );
        items.push(widgets::divider(palette).into_any_element());

        let entity = cx.entity();
        let cache_subtitle = if cache_paths > 0 {
            format!("当前缓存: {cache_size} · {cache_paths}个位置")
        } else {
            format!("当前缓存: {cache_size}")
        };
        items.push(
            self.render_item(
                palette,
                "settings-cache",
                widgets::gradient_icon(icons::TRASH.as_bytes(), 24.0, &cx.svg_renderer()),
                "清除缓存",
                &cache_subtitle,
                move |window, cx| {
                    window.blur(cx);
                    entity.update(cx, |this, cx| {
                        this.open_clear_cache(cx);
                    });
                },
            )
            .into_any_element(),
        );

        div()
            .id("settings-scroll")
            .flex_1()
            .w_full()
            .overflow_y_scroll()
            .child(div().w_full().flex().flex_col().pt(px(8.0)).children(items))
            .into_any_element()
    }

    fn render_item(
        &self,
        palette: Palette,
        id: &'static str,
        icon: AnyElement,
        title: &str,
        subtitle: &str,
        on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
    ) -> impl IntoElement {
        let handler = std::rc::Rc::new(on_click);
        div()
            .id(id)
            .w_full()
            .px(px(16.0))
            .py(px(12.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .hover(move |style| style.bg(palette.window_btn_hover))
            .on_click(move |_, window, cx| {
                handler(window, cx);
            })
            .child(div().mr(px(16.0)).child(icon))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(16.0))
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
    }
}
