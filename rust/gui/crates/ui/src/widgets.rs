//! 基础控件。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    div, img, point, px, relative, size, svg, AnyElement, App, AppContext, Bounds, ClickEvent,
    Context, Corners, DefiniteLength, DevicePixels, Div, DragMoveEvent, Element, ElementId,
    GlobalElementId, ImageSource, InspectorElementId, InteractiveElement, IntoElement, LayoutId,
    Length, MouseButton, MouseDownEvent, ObjectFit, ParentElement, Pixels, RenderImage, RenderOnce,
    Rgba, ScrollHandle, SharedString, StatefulInteractiveElement, Style, Styled, StyledImage,
    SvgRenderer, SvgSize, Window, WindowControlArea,
};

use crate::icons;
use crate::theme::{ButtonColors, Palette};

pub fn icon(data: &'static str, size: f32, color: Rgba) -> impl IntoElement {
    svg()
        .data(data.as_bytes())
        .w(px(size))
        .h(px(size))
        .aspect_ratio(1.0)
        .flex_shrink_0()
        .text_color(color)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarTarget {
    HomeInput,
    CookieInput,
}

#[derive(Clone, Copy)]
struct ScrollbarDragState {
    content_top: f32,
    grab_offset: f32,
    travel: f32,
    scroll_max: f32,
}

#[derive(Clone, Default)]
pub struct ScrollbarDrag {
    state: Rc<RefCell<Option<ScrollbarDragState>>>,
}

impl ScrollbarDrag {
    fn begin(
        &self,
        pointer_y: f32,
        bounds: Bounds<Pixels>,
        scroll_max: f32,
        viewport_height: f32,
        thumb_height: f32,
        offset: f32,
    ) -> f32 {
        let content_top = f32::from(bounds.origin.y) + 6.0;
        let visible_thumb_height = (thumb_height - 12.0).max(4.0);
        let travel = (viewport_height - thumb_height).max(0.0);
        let pointer = (pointer_y - content_top).max(0.0);
        let current_top = if scroll_max > 0.0 {
            (offset / scroll_max).clamp(0.0, 1.0) * travel
        } else {
            0.0
        };
        let grab_offset = if pointer >= current_top && pointer <= current_top + visible_thumb_height
        {
            pointer - current_top
        } else {
            visible_thumb_height * 0.5
        };
        *self.state.borrow_mut() = Some(ScrollbarDragState {
            content_top,
            grab_offset,
            travel,
            scroll_max,
        });
        if travel > 0.0 {
            ((pointer - grab_offset).clamp(0.0, travel) / travel) * scroll_max
        } else {
            0.0
        }
    }

    fn drag_to(&self, pointer_y: f32) -> Option<f32> {
        let state = (*self.state.borrow())?;
        if state.travel <= 0.0 {
            return Some(0.0);
        }
        let thumb_top =
            (pointer_y - state.content_top - state.grab_offset).clamp(0.0, state.travel);
        Some((thumb_top / state.travel) * state.scroll_max)
    }
}

pub fn vertical_scrollbar(
    cx: &mut Context<crate::AppView>,
    id: impl Into<ElementId>,
    target: ScrollbarTarget,
    drag: ScrollbarDrag,
    scroll_handle: ScrollHandle,
    palette: Palette,
) -> AnyElement {
    let scroll_max = f32::from(scroll_handle.max_offset().y);
    let viewport_height = f32::from(scroll_handle.bounds().size.height);
    if scroll_max <= 4.0 || viewport_height <= 0.0 {
        return div().into_any_element();
    }

    let thumb_height = (viewport_height * viewport_height / (viewport_height + scroll_max))
        .clamp(28.0, viewport_height);
    let travel = (viewport_height - thumb_height).max(0.0);
    let offset = f32::from(-scroll_handle.offset().y).clamp(0.0, scroll_max);
    let thumb_top = if scroll_max > 0.0 {
        (offset / scroll_max).clamp(0.0, 1.0) * travel
    } else {
        0.0
    };

    let drag_for_down = drag.clone();
    let scroll_for_down = scroll_handle.clone();
    let drag_for_move = drag;
    let scroll_for_move = scroll_handle;
    div()
        .id(id)
        .absolute()
        .top_0()
        .right_0()
        .h_full()
        .w(px(12.0))
        .cursor_pointer()
        .occlude()
        .on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, window, cx| {
                let scroll = drag_for_down.begin(
                    event.position.y.into(),
                    scroll_for_down.bounds(),
                    scroll_max,
                    viewport_height,
                    thumb_height,
                    offset,
                );
                scroll_for_down.set_offset(point(px(0.0), px(-scroll)));
                window.refresh();
                cx.stop_propagation();
            },
        )
        .on_drag(target, |_, _, _, cx| cx.new(|_| gpui::EmptyView))
        .on_drag_move::<ScrollbarTarget>(cx.listener(
            move |_, event: &DragMoveEvent<ScrollbarTarget>, _, cx| {
                if *event.drag(cx) != target {
                    return;
                }
                let Some(scroll) = drag_for_move.drag_to(event.event.position.y.into()) else {
                    return;
                };
                scroll_for_move.set_offset(point(px(0.0), px(-scroll)));
                cx.notify();
                cx.stop_propagation();
            },
        ))
        .child(
            div()
                .absolute()
                .top(px(thumb_top + 6.0))
                .right(px(4.0))
                .w(px(4.0))
                .h(px((thumb_height - 12.0).max(4.0)))
                .rounded_full()
                .bg(palette.text_muted.opacity(0.75)),
        )
        .into_any_element()
}

/// 加载内嵌渐变 SVG，所有平台使用同一套成品矢量资源。
/// 按目标显示尺寸的设备像素栅格化，保证 1:1 采样不模糊。
fn gradient_image_source(data: &'static [u8], width: f32, height: f32) -> ImageSource {
    thread_local! {
        static GRADIENT_IMAGES: RefCell<HashMap<(&'static [u8], u32), Arc<RenderImage>>> =
            RefCell::new(HashMap::new());
    }

    ImageSource::Custom(Arc::new(move |window, cx| {
        let scale = window.scale_factor().round().max(1.0) as u32;
        Some(GRADIENT_IMAGES.with(|cache| {
            let key = (data, scale);
            if let Some(image) = cache.borrow().get(&key) {
                return Ok(image.clone());
            }

            let svg = cx.svg_renderer().parse_svg(data)?;
            let device_size = size(
                DevicePixels((width * scale as f32).round() as i32),
                DevicePixels((height * scale as f32).round() as i32),
            );
            let image = cx
                .svg_renderer()
                .render_parsed(&svg, SvgSize::ExactSize(device_size))?;
            cache.borrow_mut().insert(key, image.clone());
            Ok(image)
        }))
    }))
}

pub fn gradient_icon(data: &'static [u8], size: f32, _renderer: &SvgRenderer) -> AnyElement {
    img(gradient_image_source(data, size, size))
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .into_any_element()
}

pub const GRADIENT_TITLE: &[u8] = include_bytes!("../../../../assets/gradient/title.svg");
pub const GRADIENT_TITLE_WIDTH: f32 = 98.0;
pub const GRADIENT_TITLE_HEIGHT: f32 = 19.0;
pub const GRADIENT_PASTE: &[u8] = include_bytes!("../../../../assets/gradient/paste.svg");
const GRADIENT_PASTE_SVG: &str = include_str!("../../../../assets/gradient/paste.svg");
pub const GRADIENT_CLEAR: &[u8] = include_bytes!("../../../../assets/gradient/clear.svg");
const GRADIENT_CLEAR_SVG: &str = include_str!("../../../../assets/gradient/clear.svg");
pub const GRADIENT_PARSE: &[u8] = include_bytes!("../../../../assets/gradient/parse.svg");
const GRADIENT_PARSE_SVG: &str = include_str!("../../../../assets/gradient/parse.svg");
pub const GRADIENT_EXPORT_SCRIPT: &[u8] =
    include_bytes!("../../../../assets/gradient/export-script.svg");
pub const GRADIENT_BUTTON_BACKGROUND: &[u8] =
    include_bytes!("../../../../assets/gradient/button-background.svg");
pub const GRADIENT_BUTTON_ROUNDED_BACKGROUND: &[u8] =
    include_bytes!("../../../../assets/gradient/button-background-rounded.svg");
pub const GRADIENT_BUTTON_ROUNDED_8_BACKGROUND: &[u8] =
    include_bytes!("../../../../assets/gradient/button-background-rounded-8.svg");

#[derive(Clone, Copy)]
struct GradientButtonLabel {
    image: &'static [u8],
    svg: &'static str,
    width: f32,
    height: f32,
}

fn gradient_button_label(label: &str) -> Option<GradientButtonLabel> {
    match label {
        "粘贴" => Some(GradientButtonLabel {
            image: GRADIENT_PASTE,
            svg: GRADIENT_PASTE_SVG,
            width: 27.0,
            height: 13.0,
        }),
        "清空" => Some(GradientButtonLabel {
            image: GRADIENT_CLEAR,
            svg: GRADIENT_CLEAR_SVG,
            width: 26.0,
            height: 13.0,
        }),
        "解析" => Some(GradientButtonLabel {
            image: GRADIENT_PARSE,
            svg: GRADIENT_PARSE_SVG,
            width: 27.0,
            height: 13.0,
        }),
        "导出下载脚本" => Some(GradientButtonLabel {
            image: GRADIENT_EXPORT_SCRIPT,
            svg: "",
            width: 83.0,
            height: 13.0,
        }),
        _ => None,
    }
}

pub fn gradient_text(data: &'static [u8], width: f32, height: f32) -> impl IntoElement {
    img(gradient_image_source(data, width, height))
        .w(px(width))
        .h(px(height))
        .aspect_ratio(width / height)
        .object_fit(ObjectFit::Fill)
        .flex_shrink_0()
}

pub fn gradient_button_background(corner_radius: f32) -> impl IntoElement {
    let data = if corner_radius >= 12.0 {
        GRADIENT_BUTTON_ROUNDED_BACKGROUND
    } else if corner_radius >= 8.0 {
        GRADIENT_BUTTON_ROUNDED_8_BACKGROUND
    } else {
        GRADIENT_BUTTON_BACKGROUND
    };
    GradientBackground::new(data)
}

/// 按钮渐变背景：绘制时按实际 bounds 的设备像素栅格化，保证 1:1 采样不模糊。
pub struct GradientBackground {
    data: &'static [u8],
}

impl GradientBackground {
    pub fn new(data: &'static [u8]) -> Self {
        Self { data }
    }
}

impl IntoElement for GradientBackground {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for GradientBackground {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size = size(
            Length::Definite(DefiniteLength::Fraction(1.0)),
            Length::Definite(DefiniteLength::Fraction(1.0)),
        );
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let device_size = bounds.size.to_device_pixels(window.scale_factor());
        if device_size.width.0 <= 0 || device_size.height.0 <= 0 {
            return;
        }
        thread_local! {
            static BACKGROUNDS: RefCell<HashMap<(&'static [u8], (i32, i32)), Arc<RenderImage>>> =
                RefCell::new(HashMap::new());
        }
        let key = (self.data, (device_size.width.0, device_size.height.0));
        let image = BACKGROUNDS.with(|cache| {
            if let Some(image) = cache.borrow().get(&key) {
                return Some(image.clone());
            }
            let Ok(svg) = cx.svg_renderer().parse_svg(self.data) else {
                return None;
            };
            let Ok(image) = cx
                .svg_renderer()
                .render_parsed(&svg, SvgSize::ExactSize(device_size))
            else {
                return None;
            };
            cache.borrow_mut().insert(key, image.clone());
            Some(image)
        });
        let Some(image) = image else {
            return;
        };
        window
            .paint_image(bounds, bounds, Corners::all(px(0.0)), image, 0, false)
            .ok();
    }
}

/// 在渲染时获取 GPUI SVG renderer，适合在尚无 `App` 上下文的构建函数中使用。
#[derive(IntoElement)]
pub struct GradientIcon {
    data: &'static [u8],
    size: f32,
}

impl GradientIcon {
    pub fn new(data: &'static [u8], size: f32) -> Self {
        Self { data, size }
    }
}

impl RenderOnce for GradientIcon {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        gradient_icon(self.data, self.size, &cx.svg_renderer())
    }
}

fn gradient_svg_for_icon(data: &'static str) -> &'static [u8] {
    match data {
        icons::PASTE => icons::PASTE.as_bytes(),
        icons::REFRESH => icons::REFRESH.as_bytes(),
        icons::SEND => icons::SEND.as_bytes(),
        icons::COPY => icons::COPY.as_bytes(),
        icons::DOWNLOAD => icons::DOWNLOAD.as_bytes(),
        icons::TERMINAL => icons::TERMINAL.as_bytes(),
        icons::CHECK_CIRCLE => icons::CHECK_CIRCLE.as_bytes(),
        icons::CHECK => icons::CHECK.as_bytes(),
        _ => &[],
    }
}

#[derive(IntoElement)]
pub struct ActionButton {
    id: ElementId,
    label: SharedString,
    icon: Option<&'static str>,
    icon_size: f32,
    colors: ButtonColors,
    height: f32,
    corner_radius: f32,
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
            corner_radius: 4.0,
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

    pub fn corner_radius(mut self, corner_radius: f32) -> Self {
        self.corner_radius = corner_radius;
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
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = self.colors;
        let hover_bg = colors.hover_bg;
        let interactive = self.enabled && self.on_click.is_some();
        let on_click = self.on_click;
        let icon_data = self.icon;
        let icon_size = self.icon_size;
        let gradient_label = gradient_button_label(&self.label);
        let label_slot_width = gradient_label.map(|label| label.width + 2.0);

        let icon = icon_data.map(|data| {
            if !interactive || colors.gradient || colors.fg == gpui::rgb(0xffffff) {
                icon(data, icon_size, colors.fg).into_any_element()
            } else if gradient_svg_for_icon(data).is_empty() {
                icon(data, icon_size, colors.fg).into_any_element()
            } else {
                gradient_icon(gradient_svg_for_icon(data), icon_size, &cx.svg_renderer())
            }
        });
        let label = match gradient_label {
            Some(label)
                if !label.svg.is_empty()
                    && !colors.gradient
                    && colors.fg == gpui::rgb(crate::theme::PRIMARY) =>
            {
                gradient_text(label.image, label.width, label.height).into_any_element()
            }
            Some(label) if !label.svg.is_empty() => svg()
                .data(label.svg.as_bytes())
                .w(px(label.width))
                .h(px(label.height))
                .flex_shrink_0()
                .text_color(colors.fg)
                .into_any_element(),
            _ => self.label.clone().into_any_element(),
        };

        let mut label_slot = div()
            .h(px(icon_size))
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .line_height(px(icon_size))
            .child(label);
        if let Some(width) = label_slot_width {
            label_slot = label_slot.w(px(width));
        }

        let content = div()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .children(icon.map(|icon| {
                div()
                    .w(px(icon_size))
                    .h(px(icon_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(icon)
                    .into_any_element()
            }))
            .child(label_slot);

        let button = div()
            .id(self.id)
            .relative()
            .h(px(self.height))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .rounded(px(self.corner_radius))
            .overflow_hidden()
            .border_1()
            .border_color(colors.border)
            .text_color(colors.fg)
            .text_size(px(14.0))
            .whitespace_nowrap()
            .when(!colors.gradient, |el| el.bg(colors.bg))
            .when(colors.gradient, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .child(gradient_button_background(self.corner_radius)),
                )
            })
            .when(interactive, |el| {
                el.cursor_pointer()
                    .when(colors.gradient, |el| {
                        el.hover(move |style| style.opacity(0.85))
                    })
                    .when(!colors.gradient, |el| {
                        el.hover(move |style| style.bg(hover_bg))
                    })
                    .when_some(on_click, |el, handler| {
                        el.on_click(move |event, window, cx| handler(event, window, cx))
                    })
            })
            .when(!interactive, |el| el.cursor_default())
            .child(content);
        button
    }
}

/// 顶栏图标按钮（帮助 / 设置）。
#[derive(IntoElement)]
pub struct TitleIconButton {
    id: ElementId,
    data: &'static [u8],
    size: f32,
    box_size: f32,
    hover_bg: Rgba,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl TitleIconButton {
    pub fn new(
        id: impl Into<ElementId>,
        data: &'static [u8],
        _color: Rgba,
        hover_bg: Rgba,
    ) -> Self {
        Self {
            id: id.into(),
            data,
            size: 24.0,
            box_size: 32.0,
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
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
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
            .child(gradient_icon(self.data, size, &cx.svg_renderer()))
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
