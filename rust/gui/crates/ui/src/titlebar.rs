//! 自绘标题栏：系统按钮预留条 + 应用标题行（桌面端）/ 应用栏（移动端）。

use gpui::{
    div, px, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};

use gpui_ui_kit::{TooltipPlacement, WithTooltip};

use crate::paths::is_desktop;
use crate::theme::Palette;
use crate::widgets::TitleIconButton;
use crate::{
    icons, is_double_click, AppView, ResultViewMode, Route, MOBILE_APPBAR_HEIGHT,
    SYSTEM_BAR_HEIGHT, TITLE_ROW_HEIGHT,
};

fn home_logo() -> AnyElement {
    div()
        .w(px(20.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .child(crate::widgets::GradientIcon::new(icons::HOME_LOGO, 20.0))
        .into_any_element()
}

impl AppView {
    pub fn render_titlebar(
        &mut self,
        route: Route,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = self.palette();
        if !is_desktop() {
            return self.render_mobile_appbar(route, palette, cx);
        }

        let mut system_bar = div()
            .id("system-bar")
            .h(px(SYSTEM_BAR_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_between();

        let drag_strip = div().id("system-drag-strip").flex_1().h_full();
        #[cfg(target_os = "macos")]
        let drag_strip = drag_strip
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            .on_click(|event, window, _| {
                if is_double_click(event) {
                    window.zoom_window();
                }
            });
        #[cfg(target_os = "windows")]
        let drag_strip = drag_strip
            .window_control_area(gpui::WindowControlArea::Drag)
            .on_click(|event, window, _| {
                if is_double_click(event) {
                    window.zoom_window();
                }
            });

        system_bar = system_bar.child(drag_strip);

        #[cfg(target_os = "windows")]
        {
            use crate::widgets::{CaptionButton, CaptionKind};
            system_bar = system_bar.child(
                div()
                    .flex()
                    .items_center()
                    .child(CaptionButton::new(CaptionKind::Minimize, palette))
                    .child(CaptionButton::new(CaptionKind::Maximize, palette))
                    .child(CaptionButton::new(CaptionKind::Close, palette)),
            );
        }

        let title_row = match route {
            Route::Settings => self.render_settings_title_row(palette, cx),
            Route::Results => self.render_results_title_row(palette, cx),
            Route::Home => self.render_home_title_row(palette, cx),
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(system_bar)
            .child(title_row)
            .into_any_element()
    }

    fn render_home_title_row(&mut self, palette: Palette, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let theme_entity = entity.clone();
        let settings_entity = entity.clone();
        let theme_icon = self.theme_icon();
        let theme_label = self.theme_label().to_string();

        let logo = home_logo();

        let mut drag = div()
            .id("home-title-drag")
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(logo)
            .child(div().flex_shrink_0().child(crate::widgets::gradient_text(
                crate::widgets::GRADIENT_TITLE,
                crate::widgets::GRADIENT_TITLE_WIDTH,
                crate::widgets::GRADIENT_TITLE_HEIGHT,
            )));

        #[cfg(target_os = "macos")]
        {
            drag = drag
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .on_click(|event, window, _| {
                    if is_double_click(event) {
                        window.zoom_window();
                    }
                });
        }
        #[cfg(target_os = "windows")]
        {
            drag = drag
                .window_control_area(gpui::WindowControlArea::Drag)
                .on_click(|event, window, _| {
                    if is_double_click(event) {
                        window.zoom_window();
                    }
                });
        }

        div()
            .h(px(TITLE_ROW_HEIGHT))
            .w_full()
            .pl(px(32.0))
            // 右侧对齐的是图标本体（24px），按钮 32px 的 hover 盒子不算，
            // 因此少留 4px，让图标右缘与输入框右缘齐平。
            .pr(px(28.0))
            .flex()
            .items_center()
            .child(drag)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        WithTooltip::new(
                            TitleIconButton::new(
                                "theme-button",
                                theme_icon,
                                palette.primary,
                                palette.window_btn_hover,
                            )
                            .on_click(move |_, _, cx| {
                                theme_entity.update(cx, |this, cx| this.cycle_theme(cx));
                            }),
                            theme_label,
                        )
                        .placement(TooltipPlacement::Bottom),
                    )
                    .child(
                        WithTooltip::new(
                            TitleIconButton::new(
                                "settings-button",
                                icons::SETTINGS.as_bytes(),
                                palette.primary,
                                palette.window_btn_hover,
                            )
                            .on_click(move |_, _, cx| {
                                settings_entity.update(cx, |this, cx| this.open_settings(cx));
                            }),
                            "设置",
                        )
                        .placement(TooltipPlacement::Bottom),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_title_row(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity();
        let mut drag = div()
            .id("settings-title-drag")
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.text)
                    .child("设置"),
            );
        #[cfg(target_os = "macos")]
        {
            drag = drag
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .on_click(|event, window, _| {
                    if is_double_click(event) {
                        window.zoom_window();
                    }
                });
        }
        #[cfg(target_os = "windows")]
        {
            drag = drag.window_control_area(gpui::WindowControlArea::Drag);
        }

        div()
            .h(px(TITLE_ROW_HEIGHT))
            .w_full()
            .pl(px(12.0))
            .pr(px(12.0))
            .flex()
            .items_center()
            .child(
                TitleIconButton::new(
                    "back-button",
                    icons::BACK.as_bytes(),
                    palette.primary,
                    palette.window_btn_hover,
                )
                .on_click(move |_, _, cx| {
                    entity.update(cx, |this, cx| this.close_settings(cx));
                }),
            )
            .child(drag)
            .child(div().w(px(32.0)).h(px(32.0)))
            .into_any_element()
    }

    fn render_results_title_row(&mut self, palette: Palette, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let mut drag = div()
            .id("results-title-drag")
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(px(17.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.text)
                    .child("解析结果"),
            );
        #[cfg(target_os = "macos")]
        {
            drag = drag
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .on_click(|event, window, _| {
                    if is_double_click(event) {
                        window.zoom_window();
                    }
                });
        }
        #[cfg(target_os = "windows")]
        {
            drag = drag.window_control_area(gpui::WindowControlArea::Drag);
        }

        div()
            .h(px(TITLE_ROW_HEIGHT))
            .w_full()
            .pl(px(12.0))
            .pr(px(12.0))
            .flex()
            .items_center()
            .child(
                TitleIconButton::new(
                    "results-back-button",
                    icons::BACK.as_bytes(),
                    palette.primary,
                    palette.window_btn_hover,
                )
                .on_click(move |_, _, cx| {
                    entity.update(cx, |this, cx| this.close_results(cx));
                }),
            )
            .child(drag)
            .child(div().w(px(32.0)).h(px(32.0)))
            .child(
                div()
                    .absolute()
                    .right(px(12.0))
                    .top_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(self.render_result_view_toggle(palette, cx)),
            )
            .into_any_element()
    }

    /// 结果页右上角的极客版 / 体验版切换按钮：图标与文字整体可点击。
    fn render_result_view_toggle(
        &mut self,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity();
        let label = match self.result_view {
            ResultViewMode::Geek => "体验版",
            ResultViewMode::Simple => "极客版",
        };
        let color = palette.text_secondary;
        div()
            .id("result-view-toggle")
            .h(px(28.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .cursor_pointer()
            .hover(|style| style.opacity(0.6))
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.toggle_result_view(cx));
            })
            .child(
                gpui::svg()
                    .data(icons::SWITCH)
                    .text_color(color)
                    .w(px(14.0))
                    .h(px(14.0))
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(color)
                    .whitespace_nowrap()
                    .child(label),
            )
            .into_any_element()
    }

    fn render_mobile_appbar(
        &mut self,
        route: Route,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity();
        let theme_entity = entity.clone();
        let settings_entity = entity.clone();
        let theme_icon = self.theme_icon();

        let title = match route {
            Route::Settings | Route::Results => {
                let text = if route == Route::Settings {
                    "设置"
                } else {
                    "解析结果"
                };
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_size(px(17.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .child(text),
                    )
                    .into_any_element()
            }
            Route::Home => div()
                .flex_1()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(home_logo())
                .child(div().min_w(px(0.0)).child(crate::widgets::gradient_text(
                    crate::widgets::GRADIENT_TITLE,
                    crate::widgets::GRADIENT_TITLE_WIDTH,
                    crate::widgets::GRADIENT_TITLE_HEIGHT,
                )))
                .into_any_element(),
        };

        let leading = match route {
            Route::Settings => TitleIconButton::new(
                "back-button",
                icons::BACK.as_bytes(),
                palette.primary,
                palette.window_btn_hover,
            )
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.close_settings(cx));
            })
            .into_any_element(),
            Route::Results => TitleIconButton::new(
                "results-back-button",
                icons::BACK.as_bytes(),
                palette.primary,
                palette.window_btn_hover,
            )
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.close_results(cx));
            })
            .into_any_element(),
            Route::Home => div().w(px(0.0)).into_any_element(),
        };

        let (left_padding, right_padding) = match route {
            Route::Settings | Route::Results => (8.0, 8.0),
            Route::Home => (32.0, 28.0),
        };

        let trailing = if route == Route::Home {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    TitleIconButton::new(
                        "theme-button",
                        theme_icon,
                        palette.primary,
                        palette.window_btn_hover,
                    )
                    .on_click(move |_, _, cx| {
                        theme_entity.update(cx, |this, cx| this.cycle_theme(cx));
                    }),
                )
                .child(
                    TitleIconButton::new(
                        "settings-button",
                        icons::SETTINGS.as_bytes(),
                        palette.primary,
                        palette.window_btn_hover,
                    )
                    .on_click(move |_, _, cx| {
                        settings_entity.update(cx, |this, cx| this.open_settings(cx));
                    }),
                )
                .into_any_element()
        } else {
            div().w(px(32.0)).h(px(32.0)).into_any_element()
        };

        let view_toggle = if route == Route::Results {
            Some(
                div()
                    .absolute()
                    .right(px(8.0))
                    .top_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(self.render_result_view_toggle(palette, cx))
                    .into_any_element(),
            )
        } else {
            None
        };

        div()
            .h(px(MOBILE_APPBAR_HEIGHT))
            .w_full()
            .pl(px(left_padding))
            .pr(px(right_padding))
            .flex()
            .items_center()
            .child(leading)
            .child(title)
            .child(trailing)
            .children(view_toggle)
            .into_any_element()
    }
}
