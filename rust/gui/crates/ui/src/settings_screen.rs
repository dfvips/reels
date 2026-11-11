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
        self.route = Route::Settings;
        self.refresh_cache_info();
        cx.notify();
    }

    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.route = Route::Home;
        cx.notify();
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

    pub fn theme_icon(&self) -> &'static str {
        match self.settings.theme_mode.as_str() {
            "light" => icons::SUN,
            "dark" => icons::MOON,
            _ => icons::CONTRAST,
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

    pub fn clear_cache(&mut self, cx: &mut Context<Self>) {
        let result = crate::download::clear_cache(&self.paths.cache_dir);
        self.refresh_cache_info();
        let (success, message, details) = match result {
            Ok(()) => (
                true,
                "缓存清理完成".to_string(),
                self.cache_paths
                    .iter()
                    .map(|path| format!("🗑️ {path}"))
                    .collect(),
            ),
            Err(error) => (
                false,
                format!("清除缓存失败: {error}"),
                vec![format!("❌ {error}")],
            ),
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
                    icons::FOLDER,
                    "存储设置",
                    "设置视频保存的默认位置",
                    move |cx| {
                        entity.update(cx, |this, cx| {
                            this.open_save_location_dialog(cx);
                        });
                    },
                )
                .into_any_element(),
            );
            items.push(widgets::divider(palette).into_any_element());
        }

        let entity = cx.entity();
        let cookie_status = if self.settings.xhs_cookie.trim().is_empty() {
            "未设置（默认留白）"
        } else {
            "已设置（用于小红书解析）"
        };
        items.push(
            self.render_item(
                palette,
                "settings-cookie",
                icons::LOCK,
                "Cookie 设置",
                &format!("仅用于小红书 · {cookie_status}"),
                move |cx| {
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
                icons::HELP,
                "使用说明",
                "查看支持的链接格式与使用方式",
                move |cx| {
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
                icons::INFO,
                "关于",
                &format!("应用版本: {APP_VERSION}"),
                move |cx| {
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
                icons::TRASH,
                "清除缓存",
                &cache_subtitle,
                move |cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = Some(Dialog::ClearCache);
                        cx.notify();
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
        icon: &'static str,
        title: &str,
        subtitle: &str,
        on_click: impl Fn(&mut gpui::App) + 'static,
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
            .on_click(move |_, _, cx| {
                handler(cx);
            })
            .child(
                div()
                    .mr(px(16.0))
                    .child(widgets::icon(icon, 24.0, palette.primary)),
            )
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
