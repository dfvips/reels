//! 基础控件。

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, relative, svg, AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled,
    Window, WindowControlArea,
};

use crate::icons;
use crate::theme::{ButtonColors, Palette};

pub fn icon(data: &'static str, size: f32, color: Rgba) -> impl IntoElement {
    svg()
        .data(data.as_bytes())
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(color)
}

#[derive(IntoElement)]
pub struct ActionButton {
    id: ElementId,
    label: SharedString,
    icon: Option<&'static str>,
    icon_size: f32,
    colors: ButtonColors,
    height: f32,
    enabled: bool,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl ActionButton {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        colors: ButtonColors,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            icon_size: 16.0,
            colors,
            height: 42.0,
            enabled: true,
            on_click: None,
        }
    }

    pub fn icon(mut self, data: &'static str, size: f32) -> Self {
        self.icon = Some(data);
        self.icon_size = size;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ActionButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.colors;
        let hover_bg = colors.hover_bg;
        let interactive = self.enabled && self.on_click.is_some();
        let on_click = self.on_click;
        let icon_data = self.icon;
        let icon_size = self.icon_size;

        div()
            .id(self.id)
            .h(px(self.height))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.bg)
            .text_color(colors.fg)
            .text_size(px(14.0))
            .whitespace_nowrap()
            .when(interactive, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(hover_bg))
                    .when_some(on_click, |el, handler| {
                        el.on_click(move |event, window, cx| handler(event, window, cx))
                    })
            })
            .when(!interactive, |el| el.cursor_default())
            .children(icon_data.map(|data| icon(data, icon_size, colors.fg)))
            .child(self.label)
    }
}

/// 顶栏图标按钮（帮助 / 设置）。
#[derive(IntoElement)]
pub struct TitleIconButton {
    id: ElementId,
    data: &'static str,
    size: f32,
    box_size: f32,
    color: Rgba,
    hover_bg: Rgba,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl TitleIconButton {
    pub fn new(id: impl Into<ElementId>, data: &'static str, color: Rgba, hover_bg: Rgba) -> Self {
        Self {
            id: id.into(),
            data,
            size: 24.0,
            box_size: 32.0,
            color,
            hover_bg,
            on_click: None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn box_size(mut self, box_size: f32) -> Self {
        self.box_size = box_size;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for TitleIconButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let on_click = self.on_click;
        let hover_bg = self.hover_bg;
        let size = self.size;
        div()
            .id(self.id)
            .w(px(self.box_size))
            .h(px(self.box_size))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .when_some(on_click, |el, handler| {
                el.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(icon(self.data, size, self.color))
    }
}

/// Windows 自绘标题栏按钮：交给系统处理最小化 / 最大化还原 / 关闭。
#[derive(IntoElement)]
pub struct CaptionButton {
    kind: CaptionKind,
    palette: Palette,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CaptionKind {
    Minimize,
    Maximize,
    Close,
}

impl CaptionButton {
    pub fn new(kind: CaptionKind, palette: Palette) -> Self {
        Self { kind, palette }
    }
}

impl RenderOnce for CaptionButton {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        let group = match self.kind {
            CaptionKind::Minimize => "win-min",
            CaptionKind::Maximize => "win-max",
            CaptionKind::Close => "win-close",
        };
        let icon_data = match self.kind {
            CaptionKind::Minimize => icons::WIN_MINIMIZE,
            CaptionKind::Maximize => {
                if window.is_maximized() {
                    icons::WIN_RESTORE
                } else {
                    icons::WIN_MAXIMIZE
                }
            }
            CaptionKind::Close => icons::WIN_CLOSE,
        };
        let control_area = match self.kind {
            CaptionKind::Minimize => WindowControlArea::Min,
            CaptionKind::Maximize => WindowControlArea::Max,
            CaptionKind::Close => WindowControlArea::Close,
        };
        let icon_color = palette.window_icon;
        let is_close = self.kind == CaptionKind::Close;
        let hover_bg = if is_close {
            palette.close_hover
        } else {
            palette.window_btn_hover
        };
        let normal_bg = gpui::rgba(0x00000000);

        let icon_size = if is_close { 16.0 } else { 16.0 };

        div()
            .id(SharedString::from(format!("caption-{group}")))
            .group(group)
            .window_control_area(control_area)
            .w(px(46.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .bg(normal_bg)
            .hover(move |style| style.bg(hover_bg))
            .child(
                svg()
                    .data(icon_data.as_bytes())
                    .w(px(icon_size))
                    .h(px(icon_size))
                    .text_color(icon_color)
                    .when(is_close, |el| {
                        el.group_hover(group, |style| style.text_color(gpui::rgb(0xffffff)))
                    }),
            )
    }
}

/// 简单的旋转指示器（由 8 个圆点组成）。
#[derive(IntoElement)]
pub struct Spinner {
    pub phase: f32,
    pub color: Rgba,
    pub size: f32,
}

impl RenderOnce for Spinner {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut dots = Vec::new();
        let radius = self.size / 2.0 - 2.0;
        let dot = 3.0;
        for index in 0..8 {
            let angle = index as f32 / 8.0 * std::f32::consts::TAU;
            let x = self.size / 2.0 + radius * angle.cos() - dot / 2.0;
            let y = self.size / 2.0 + radius * angle.sin() - dot / 2.0;
            let phase = (self.phase + index as f32 / 8.0) % 1.0;
            let alpha = 0.15 + 0.85 * phase;
            dots.push(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(dot))
                    .h(px(dot))
                    .rounded_full()
                    .bg(gpui::Hsla {
                        a: alpha,
                        ..self.color.into()
                    }),
            );
        }
        div()
            .relative()
            .w(px(self.size))
            .h(px(self.size))
            .children(dots)
    }
}

#[derive(IntoElement)]
pub struct ProgressBar {
    pub value: f32,
    pub color: Rgba,
    pub track: Rgba,
    pub height: f32,
}

impl RenderOnce for ProgressBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let value = self.value.clamp(0.0, 1.0);
        div()
            .w_full()
            .h(px(self.height))
            .rounded(px(self.height / 2.0))
            .bg(self.track)
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(value))
                    .rounded(px(self.height / 2.0))
                    .bg(self.color),
            )
    }
}

pub fn card(dark: bool) -> Div {
    let palette = Palette::new(dark);
    div()
        .flex()
        .flex_col()
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(palette.bg)
}

pub fn divider(palette: Palette) -> impl IntoElement {
    div().h(px(1.0)).w_full().bg(palette.divider)
}

pub fn any(element: impl IntoElement) -> AnyElement {
    element.into_any_element()
}

pub fn hover_bg_for(palette: Palette) -> Rgba {
    palette.window_btn_hover
}
