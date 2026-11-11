//! 对话框：帮助、关于、主题、存储、缓存、下载确认 / 进度 / 完成。

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AnyElement, App, ClickEvent, Context, Div, FontWeight, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use douyin_input::{Input, InputSize};
use std::path::PathBuf;

use crate::home::download_confirm_message;
use crate::paths::{format_size, is_desktop, open_directory};
use crate::theme::Palette;
use crate::widgets::{self, ProgressBar};
use crate::{icons, AppView, Dialog, ToastKind, APP_NAME, APP_VERSION};

const SUPPORTED_FORMATS: [&str; 7] = [
    "5.12 复制打开抖音，看看【赵今麦angel的作品】小林～拍照📷# 剧剧抖有戏  https://v.douyin.com/HUbJB4-GQdc/ 03/26 e@b.nD yGi:/ ",
    "https://v.douyin.com/L8tSCYe-sC4",
    "https://www.douyin.com/video/7588361322312650038",
    "https://m.douyin.com/share/video/7588361322312650038",
    "https://www.iesdouyin.com/share/video/7588361322312650038",
    "https://www.douyin.com/jingxuan?modal_id=7701193143558901018",
    "https://www.douyin.com/jingxuan?modal_id=7674934407219793171",
];
const SUPPORTED_XHS_FORMATS: [&str; 5] = [
    "75 【新巡演，整起！^_^ - 张杰 | 小红书 - 你的生活兴趣社区】 😆 RC8p7DRyJZO8KBy 😆 https://www.xiaohongshu.com/discovery/item/6a9e414d000000002a02d317?xsec_token=ABxxxx",
    "https://www.xiaohongshu.com/discovery/item/6a9e414d000000002a02d317?xsec_token=ABxxxx",
    "bloc1✖️愤怒攀岩👊😡👊 今天怒气值max，状态和能力... https://xhslink.cn/o/8C9r0KXJsjj 把口令复制下来，进入【小红书】看笔记~",
    "https://www.xiaohongshu.com/explore/6644e3a8000000001e01cc4d?xsec_token=ABxxxx",
    "https://www.xiaohongshu.com/user/profile/65c5fd5600000000110173a5/6773508e0000000009015fa0?xsec_token=ABxxxx",
];

impl AppView {
    pub fn render_dialog(
        &mut self,
        palette: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.dialog.clone()?;
        let width: f32 = window.bounds().size.width.into();
        let card_width = (width - 48.0).clamp(280.0, 480.0);
        let card = match dialog {
            Dialog::Help => self.dialog_help(palette, card_width, cx),
            Dialog::About => self.dialog_about(palette, card_width, cx),
            Dialog::SaveLocation => self.dialog_save_location(palette, card_width, cx),
            Dialog::XhsCookie => self.dialog_xhs_cookie(palette, card_width, cx),
            Dialog::ClearCache => self.dialog_clear_cache(palette, card_width, cx),
            Dialog::ClearCacheResult {
                success,
                message,
                details,
            } => self.dialog_clear_result(palette, card_width, success, message, details, cx),
            Dialog::DownloadConfirm => self.dialog_download_confirm(palette, card_width, cx),
            Dialog::DownloadProgress => self.dialog_download_progress(palette, card_width, cx),
            Dialog::DownloadSummary { files } => {
                self.dialog_download_summary(palette, card_width, files, cx)
            }
        };

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::rgba(0x00000066))
                .occlude()
                .child(card)
                .into_any_element(),
        )
    }

    fn dialog_card(&self, palette: Palette, width: f32) -> Div {
        div()
            .w(px(width))
            .max_h(px(600.0))
            .rounded(px(24.0))
            .bg(if palette.dark {
                gpui::rgb(0x2b2b2b)
            } else {
                gpui::rgb(0xffffff)
            })
            .p(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .text_color(palette.text)
    }

    fn dialog_title(&self, palette: Palette, text: &str, size: f32) -> Div {
        div()
            .text_size(px(size))
            .font_weight(FontWeight::MEDIUM)
            .text_color(palette.text)
            .child(text.to_string())
    }

    fn action_row(&self) -> Div {
        div().flex().justify_end().gap(px(12.0)).mt(px(8.0))
    }

    fn text_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        primary: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Stateful<Div> {
        let color = palette.primary;
        let hover = if palette.dark {
            gpui::rgba(0xffffff14)
        } else {
            gpui::rgba(0x0000000f)
        };
        div()
            .id(id)
            .h(px(38.0))
            .w(px(128.0))
            .px(px(24.0))
            .rounded_full()
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(14.0))
            .font_weight(if primary {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .text_color(color)
            .hover(move |style| style.bg(hover))
            .on_click(handler)
            .child(label.to_string())
    }

    fn primary_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Stateful<Div> {
        let hover = gpui::rgb(0x4f97fa);
        div()
            .id(id)
            .h(px(38.0))
            .w(px(128.0))
            .px(px(24.0))
            .rounded_full()
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .bg(palette.primary)
            .text_color(gpui::rgb(0xffffff))
            .text_size(px(14.0))
            .font_weight(FontWeight::MEDIUM)
            .hover(move |style| style.bg(hover))
            .on_click(handler)
            .child(label.to_string())
    }

    fn outline_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Stateful<Div> {
        let colors = palette.export_button();
        let hover = colors.hover_bg;
        div()
            .id(id)
            .h(px(38.0))
            .w(px(128.0))
            .px(px(24.0))
            .rounded_full()
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(colors.border)
            .bg(colors.bg)
            .text_color(colors.fg)
            .text_size(px(14.0))
            .hover(move |style| style.bg(hover))
            .on_click(handler)
            .child(label.to_string())
    }

    fn plain_action(
        &self,
        palette: Palette,
        id: &'static str,
        label: &str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Stateful<Div> {
        let (border, fg, hover) = if palette.dark {
            (
                gpui::rgb(0x4c4d4f),
                palette.text_secondary,
                gpui::rgba(0xffffff14),
            )
        } else {
            (
                gpui::rgb(0xdcdfe6),
                palette.text_secondary,
                gpui::rgba(0x0000000f),
            )
        };
        div()
            .id(id)
            .h(px(38.0))
            .w(px(128.0))
            .px(px(24.0))
            .rounded_full()
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(border)
            .text_color(fg)
            .text_size(px(14.0))
            .hover(move |style| style.bg(hover))
            .on_click(handler)
            .child(label.to_string())
    }

    fn dialog_help(&self, palette: Palette, width: f32, cx: &mut Context<Self>) -> Div {
        let entity = cx.entity();
        let format_rows = |items: &[&str]| -> Vec<gpui::AnyElement> {
            items
                .iter()
                .map(|format| {
                    div()
                        .flex()
                        .items_start()
                        .gap(px(8.0))
                        .child(div().mt(px(2.0)).child(widgets::icon(
                            icons::CHECK_CIRCLE,
                            16.0,
                            gpui::rgb(0x2e7d32),
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .font_family(crate::theme::mono_font())
                                .text_size(px(13.0))
                                .text_color(palette.primary)
                                .child((*format).to_string()),
                        )
                        .into_any_element()
                })
                .collect()
        };

        self.dialog_card(palette, width)
            .child(self.dialog_title(palette, "使用说明", 18.0))
            .child(
                div()
                    .id("help-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(palette.text)
                                    .child("支持解析抖音以下链接格式："),
                            )
                            .children(format_rows(&SUPPORTED_FORMATS)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(palette.text)
                                    .child("支持解析小红书笔记（视频 / 图文），以下格式均支持："),
                            )
                            .children(format_rows(&SUPPORTED_XHS_FORMATS)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.0))
                            .child(widgets::icon(icons::LINK, 16.0, palette.primary))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.0))
                                    .text_color(palette.text_muted)
                                    .child("支持批量粘贴多个抖音 / 小红书分享链接，系统会自动提取所有有效链接。"),
                            ),
                    ),
            )
            .child(self.action_row().child(self.text_action(
                palette,
                "help-close",
                "知道了",
                true,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }

    fn dialog_about(&self, palette: Palette, width: f32, cx: &mut Context<Self>) -> Div {
        let entity = cx.entity();
        self.dialog_card(palette, width)
            .child(self.dialog_title(palette, "关于", 24.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text)
                            .child(format!("{APP_NAME}工具")),
                    )
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(palette.text)
                            .child(format!("版本: {APP_VERSION}")),
                    )
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(palette.text)
                            .child("功能: 解析抖音视频链接，获取视频下载地址"),
                    )
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(palette.text)
                            .child("注意: 仅用于学习和研究目的"),
                    ),
            )
            .child(self.action_row().child(self.text_action(
                palette,
                "about-close",
                "确定",
                true,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }

    pub fn open_save_location_dialog(&mut self, cx: &mut Context<Self>) {
        self.location_dialog_directory = if self.settings.last_directory.is_empty() {
            self.paths.default_download_dir.display().to_string()
        } else {
            self.settings.last_directory.clone()
        };
        self.location_dialog_remember =
            self.settings.skip_location_prompt || self.settings.remember_location;
        self.dialog = Some(Dialog::SaveLocation);
        cx.notify();
    }

    pub fn open_xhs_cookie_dialog(&mut self, cx: &mut Context<Self>) {
        self.xhs_cookie_input = self.settings.xhs_cookie.clone();
        self.dialog = Some(Dialog::XhsCookie);
        cx.notify();
    }

    fn dialog_xhs_cookie(&mut self, palette: Palette, width: f32, cx: &mut Context<Self>) -> Div {
        let input_entity = cx.entity();
        let cancel_entity = cx.entity();
        let save_entity = cx.entity();
        let input = Input::new("xhs-cookie-input")
            .multiline(true)
            .value(self.xhs_cookie_input.clone())
            .placeholder("粘贴小红书网页 Cookie（可选，默认留白）")
            .size(InputSize::Lg)
            .bg_color(palette.result_bg)
            .border_color(gpui::rgba(0x00000000))
            .text_color(palette.text)
            .placeholder_color(palette.hint)
            .on_text_change(move |text, _, cx| {
                input_entity.update(cx, |this, cx| {
                    if this.xhs_cookie_input != text {
                        this.xhs_cookie_input = text.to_string();
                        cx.notify();
                    }
                });
            });
        self.dialog_card(palette, width)
            .child(self.dialog_title(palette, "Cookie 设置", 22.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(palette.text_muted)
                            .child(
                                "默认留白即可正常解析；如遇链接需要登录，可粘贴浏览器中小红书站点的 Cookie。",
                            ),
                    )
                    .child(input),
            )
            .child(
                self.action_row()
                    .child(self.outline_action(palette, "cookie-cancel", "取消", {
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        }
                    }))
                    .child(self.primary_action(palette, "cookie-save", "保存", {
                        move |_, _, cx| {
                            save_entity.update(cx, |this, cx| {
                                this.settings.xhs_cookie = this.xhs_cookie_input.trim().to_string();
                                this.save_settings();
                                this.dialog = None;
                                this.toast("Cookie 设置已保存", ToastKind::Success, cx);
                            });
                        }
                    })),
            )
    }

    fn dialog_save_location(
        &mut self,
        palette: Palette,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let current = if self.location_dialog_directory.is_empty() {
            "未设置".to_string()
        } else {
            self.location_dialog_directory.clone()
        };
        let has_location = !self.location_dialog_directory.is_empty();
        let remember = self.location_dialog_remember;
        let change_entity = cx.entity();
        let reset_entity = cx.entity();
        let toggle_entity = cx.entity();
        let cancel_entity = cx.entity();
        let save_entity = cx.entity();

        let checkbox = div()
            .w(px(20.0))
            .h(px(20.0))
            .rounded(px(3.0))
            .border_1()
            .border_color(if remember {
                palette.primary
            } else {
                palette.text_muted
            })
            .bg(if remember {
                palette.primary
            } else {
                gpui::rgba(0x00000000)
            })
            .flex()
            .items_center()
            .justify_center()
            .when(remember, |el| {
                el.child(widgets::icon(icons::CHECK, 14.0, gpui::rgb(0xffffff)))
            });

        self.dialog_card(palette, width)
            .child(self.dialog_title(palette, "存储设置", 22.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(div().text_size(px(14.0)).child("当前保存位置:"))
                    .child(
                        div()
                            .w_full()
                            .p(px(12.0))
                            .rounded(px(6.0))
                            .bg(palette.result_bg)
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(if has_location {
                                palette.text
                            } else {
                                palette.text_muted
                            })
                            .whitespace_normal()
                            .child(current),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.0))
                    .child(
                        div().flex_1().child(
                            self.primary_action(
                                palette,
                                "location-change",
                                "更改",
                                move |_, _, cx| {
                                    change_entity.update(cx, |this, cx| {
                                        let initial = if this.location_dialog_directory.is_empty() {
                                            None
                                        } else {
                                            Some(PathBuf::from(
                                                &this.location_dialog_directory,
                                            ))
                                        };
                                        cx.spawn(async move |weak: WeakEntity<AppView>, cx| {
                                            let picked = crate::home::pick_directory_blocking(
                                                initial.as_deref(),
                                            );
                                            let _ = weak.update(cx, |this, cx| {
                                                if let Some(directory) = picked {
                                                    this.location_dialog_directory =
                                                        directory.display().to_string();
                                                    cx.notify();
                                                }
                                            });
                                        })
                                        .detach();
                                    });
                                },
                            )
                            .w_full(),
                        ),
                    )
                    .child(
                        div().flex_1().child(
                            self.outline_action(
                                palette,
                                "location-reset",
                                "重置",
                                move |_, _, cx| {
                                    reset_entity.update(cx, |this, cx| {
                                        this.location_dialog_directory =
                                            this.paths.default_download_dir.display().to_string();
                                        this.location_dialog_remember = false;
                                        cx.notify();
                                    });
                                },
                            )
                            .w_full(),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(div().text_size(px(14.0)).child("下载选项:"))
                    .child(
                        div()
                            .id("location-remember-row")
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_between()
                            .rounded_full()
                            .cursor_pointer()
                            .px(px(4.0))
                            .py(px(4.0))
                            .on_click(move |_, _, cx| {
                                toggle_entity.update(cx, |this, cx| {
                                    this.location_dialog_remember = !this.location_dialog_remember;
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(palette.text)
                                            .child("不再提示保存位置"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(palette.text_muted)
                                            .child("下次直接保存到此位置"),
                                    ),
                            )
                            .child(checkbox),
                    ),
            )
            .child(
                self.action_row()
                    .child(self.text_action(
                        palette,
                        "location-cancel",
                        "取消",
                        false,
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "location-save",
                        "保存",
                        move |_, _, cx| {
                            save_entity.update(cx, |this, cx| {
                                let directory = this.location_dialog_directory.trim().to_string();
                                if directory.is_empty() {
                                    this.toast("请先选择保存位置", ToastKind::Error, cx);
                                    return;
                                }
                                this.settings.last_directory = directory;
                                this.settings.skip_location_prompt = this.location_dialog_remember;
                                this.settings.remember_location = this.location_dialog_remember;
                                this.save_settings();
                                this.dialog = None;
                                this.toast("设置已保存", ToastKind::Success, cx);
                            });
                        },
                    )),
            )
    }

    fn dialog_clear_cache(&self, palette: Palette, width: f32, cx: &mut Context<Self>) -> Div {
        let cancel_entity = cx.entity();
        let clear_entity = cx.entity();
        let cache_size = format_size(self.cache_size);
        let paths: Vec<AnyElement> = self
            .cache_paths
            .iter()
            .map(|path| {
                div()
                    .text_size(px(11.0))
                    .text_color(palette.text_muted)
                    .child(format!("• {path}"))
                    .into_any_element()
            })
            .collect();

        self.dialog_card(palette, width)
            .child(self.dialog_title(palette, "清除缓存", 20.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .child("确定要清除所有缓存数据吗？"),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(palette.text_secondary)
                            .child(format!("当前缓存大小: {cache_size}")),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(palette.text_muted)
                            .child("将清理以下位置的缓存:"),
                    )
                    .children(paths),
            )
            .child(
                self.action_row()
                    .child(self.text_action(
                        palette,
                        "cache-cancel",
                        "取消",
                        false,
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "cache-clear",
                        "清理",
                        move |_, _, cx| {
                            clear_entity.update(cx, |this, cx| this.clear_cache(cx));
                        },
                    )),
            )
    }

    fn dialog_clear_result(
        &self,
        palette: Palette,
        width: f32,
        success: bool,
        message: String,
        details: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let entity = cx.entity();
        let detail_elements: Vec<AnyElement> = details
            .iter()
            .map(|detail| {
                let color = if detail.contains('❌') {
                    gpui::rgb(0xd32f2f)
                } else if detail.contains('🗑') {
                    gpui::rgb(0x2e7d32)
                } else {
                    palette.text_muted
                };
                div()
                    .text_size(px(12.0))
                    .text_color(color)
                    .child(detail.clone())
                    .into_any_element()
            })
            .collect();

        self.dialog_card(palette, width)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(widgets::icon(
                        if success {
                            icons::CHECK_CIRCLE
                        } else {
                            icons::INFO
                        },
                        24.0,
                        if success {
                            gpui::rgb(0x2e7d32)
                        } else {
                            gpui::rgb(0xd32f2f)
                        },
                    ))
                    .child(self.dialog_title(
                        palette,
                        if success {
                            "清理完成"
                        } else {
                            "清理失败"
                        },
                        20.0,
                    )),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(if success {
                        gpui::rgb(0x2e7d32)
                    } else {
                        gpui::rgb(0xd32f2f)
                    })
                    .child(message),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .children(detail_elements),
            )
            .child(self.action_row().child(self.text_action(
                palette,
                "clean-ok",
                "确定",
                true,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.dialog = None;
                        cx.notify();
                    });
                },
            )))
    }

    fn dialog_download_confirm(&self, palette: Palette, width: f32, cx: &mut Context<Self>) -> Div {
        let cancel_entity = cx.entity();
        let start_entity = cx.entity();
        let message = download_confirm_message(self.results.len());

        self.dialog_card(palette, width)
            .child(self.dialog_title(palette, "下载确认", 20.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(22.0))
                    .text_color(palette.text)
                    .child(message),
            )
            .child(
                self.action_row()
                    .child(self.plain_action(
                        palette,
                        "download-cancel",
                        "取消",
                        move |_, _, cx| {
                            cancel_entity.update(cx, |this, cx| {
                                this.dialog = None;
                                cx.notify();
                            });
                        },
                    ))
                    .child(self.primary_action(
                        palette,
                        "download-start",
                        "开始下载",
                        move |_, _, cx| {
                            start_entity.update(cx, |this, cx| this.confirm_download(cx));
                        },
                    )),
            )
    }

    fn dialog_download_progress(
        &self,
        palette: Palette,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let close_entity = cx.entity();
        let (completed, total, overall, current_index, current_progress, downloaded, total_size) =
            if let Some(ui) = self.download.as_ref() {
                (
                    ui.completed_count(),
                    ui.total,
                    ui.overall_progress(),
                    ui.current_index().unwrap_or(0),
                    ui.current_progress(),
                    ui.total_downloaded(),
                    ui.total_size(),
                )
            } else {
                (0, 0, 0.0, 0, 0.0, 0, 0)
            };
        let all_completed = total > 0 && completed == total;
        let status = if all_completed {
            "所有下载完成！正在关闭进度窗口..."
        } else {
            "正在顺序下载..."
        };
        let current_size = self
            .download
            .as_ref()
            .and_then(|ui| ui.downloaded.get(current_index).copied())
            .unwrap_or(0);
        let current_total = self
            .download
            .as_ref()
            .and_then(|ui| ui.total_bytes.get(current_index).copied())
            .unwrap_or(0);

        self.dialog_card(palette, width)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.dialog_title(palette, "下载进度", 20.0))
                    .child(
                        div()
                            .id("progress-close")
                            .w(px(28.0))
                            .h(px(28.0))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(move |style| style.bg(palette.window_btn_hover))
                            .on_click(move |_, _, cx| {
                                close_entity.update(cx, |this, cx| this.cancel_download(cx));
                            })
                            .child(widgets::icon(icons::CLOSE_SMALL, 18.0, palette.text)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(palette.text)
                                    .child("总体进度:"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(gpui::rgb(0x2e7d32))
                                    .child(format!("{completed}/{total} 已完成")),
                            ),
                    )
                    .child(ProgressBar {
                        value: overall,
                        color: palette.primary,
                        track: palette.divider,
                        height: 8.0,
                    })
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(format!("{:.1}%", overall * 100.0)),
                            )
                            .child(div().text_size(px(12.0)).child(format!(
                                "{} / {}",
                                format_size(downloaded),
                                if total_size > 0 {
                                    format_size(total_size)
                                } else {
                                    "计算中...".to_string()
                                }
                            ))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(div().text_size(px(12.0)).child(format!(
                        "当前视频 ({}/{}):",
                        current_index + 1,
                        total
                    )))
                    .child(ProgressBar {
                        value: current_progress,
                        color: palette.primary,
                        track: palette.divider,
                        height: 8.0,
                    })
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .child(format!("{:.1}%", current_progress * 100.0)),
                            )
                            .child(div().text_size(px(12.0)).child(format!(
                                "{} / {}",
                                format_size(current_size),
                                format_size(current_total.max(current_size))
                            ))),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(if all_completed {
                        gpui::rgb(0x2e7d32)
                    } else {
                        palette.text_muted
                    })
                    .child(status),
            )
    }

    fn dialog_download_summary(
        &self,
        palette: Palette,
        width: f32,
        files: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Div {
        let close_entity = cx.entity();
        let first_directory = files
            .first()
            .and_then(|file| file.parent().map(|p| p.to_path_buf()));
        let mut entries: Vec<AnyElement> = files
            .iter()
            .take(5)
            .map(|file| {
                let name = file
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                div()
                    .text_size(px(12.0))
                    .text_color(palette.text)
                    .child(format!("• {name}"))
                    .into_any_element()
            })
            .collect();
        if files.len() > 5 {
            entries.push(
                div()
                    .text_size(px(12.0))
                    .text_color(palette.text_muted)
                    .child(format!("... 以及 {} 个文件", files.len() - 5))
                    .into_any_element(),
            );
        }

        let card = self
            .dialog_card(palette, width)
            .child(self.dialog_title(palette, "下载完成", 20.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .child(format!("已成功下载 {} 个视频文件：", files.len())),
            )
            .child(div().flex().flex_col().gap(px(4.0)).children(entries));

        let mut actions = self.action_row().child(self.plain_action(
            palette,
            "summary-close",
            "关闭",
            move |_, _, cx| {
                close_entity.update(cx, |this, cx| {
                    this.dialog = None;
                    cx.notify();
                });
            },
        ));

        if is_desktop() {
            if let Some(directory) = first_directory {
                actions = actions.child(self.primary_action(
                    palette,
                    "summary-open",
                    "打开文件夹",
                    move |_, _, _| {
                        open_directory(&directory);
                    },
                ));
            }
        }

        card.child(actions)
    }
}
