//! Input 组件（自 gpui-ui-kit 0.11 移植的分支，修复多行上下键导航）。
//!
//! Text input field with optional label, placeholder, and validation.
//!
//! Features:
//! - Full keyboard text editing support (self-contained)
//! - Click to focus and start editing
//! - Enter to confirm, Escape to cancel
//! - Cursor navigation and text selection
//! - Mouse drag to select text, double-click to select word
//! - Clipboard support: Cmd+C (copy), Cmd+X (cut), Cmd+V (paste), Cmd+A (select all)
//! - Emacs-style keybindings (Ctrl+A/E/K/U/W/H/D/F/B)
//! - Disabled and readonly states
//!
//! # Simple Usage
//!
//! The Input component handles all focus and keyboard events internally.
//! Just provide callbacks for changes:
//!
//! ```ignore
//! Input::new("my-input")
//!     .value(current_value)
//!     .placeholder("Enter text...")
//!     .on_change(|new_value, _window, _cx| {
//!         println!("Value changed to: {}", new_value);
//!     })
//!     .on_text_change(|text, _window, _cx| {
//!         // Called on every keystroke (optional, for live updates)
//!         println!("Current text: {}", text);
//!     })
//! ```
//!
//! # Thread-Local State Pattern
//!
//! This component uses `thread_local!` storage to persist focus handles and
//! edit state across renders. This is necessary because GPUI's `RenderOnce`
//! components are recreated on each render, but we need state to persist:
//!
//! - **Focus handles**: Must be the same instance across renders or focus is lost
//! - **Edit state**: Cursor position, text, and selection must persist during editing
//!
//! ## Memory Considerations
//!
//! The thread-local `HashMap` entries grow as new element IDs are used and are
//! never automatically cleaned up. For most applications this is fine because:
//! - Element IDs are typically static or part of a bounded set
//! - The stored data is small (`FocusHandle`, `EditState`)
//!
//! If you have dynamic element IDs (e.g., from a virtualized list), consider:
//! 1. Using a stable ID scheme that reuses IDs
//! 2. Calling `cleanup_input_state(id)` when components are removed
//!
//! ## Cleanup Function
//!
//! To manually clean up state for a removed element:
//! ```rust,ignore
//! cleanup_input_state(&element_id);
//! ```

use access::{AriaProps, AriaRole, AriaState, apply_native_accessibility};
use gpui::prelude::{
    FluentBuilder, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled,
};
use gpui::{
    AnyElement, App, AppContext, Bounds, ClipboardItem, Context, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, FontWeight, GlobalElementId,
    InspectorElementId, KeyDownEvent, LayoutId, LongPressEvent, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, Rgba, SharedString, Subscription,
    TouchPhase, UTF16Selection, WeakEntity, Window, div, px,
};
use gpui_ui_kit::theme::{Theme, ThemeState};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

thread_local! {
    static FOCUS_HANDLES: RefCell<HashMap<ElementId, FocusHandle>> = RefCell::new(HashMap::new());
}
thread_local! {
    static EDIT_STATES: RefCell<HashMap<ElementId, Rc<RefCell<EditState>>>> = RefCell::new(HashMap::new());
}
thread_local! {
    static TEXT_ORIGINS: RefCell<HashMap<ElementId, f32>> = RefCell::new(HashMap::new());
}
thread_local! {
    static FOCUS_SUBS: RefCell<HashMap<ElementId, Subscription>> = RefCell::new(HashMap::new());
}
thread_local! {
    // Cached render entities so repeated renders reuse the same GPUI entity.
    // Stored as weak references so the entities can be dropped when no longer
    // referenced by the element tree, avoiding leaked-handle panics in tests.
    static INPUT_ENTITIES: RefCell<HashMap<ElementId, WeakEntity<InputEntity>>> =
        RefCell::new(HashMap::new());
}
#[cfg(target_os = "ios")]
thread_local! {
    static NATIVE_EDITOR_TEXT_ECHOES: RefCell<HashMap<String, String>> =
        RefCell::new(HashMap::new());
}
#[cfg(target_os = "android")]
thread_local! {
    static NATIVE_EDITOR_SCROLL_HANDLE: RefCell<Option<gpui::ScrollHandle>> =
        const { RefCell::new(None) };
}

mod access;
mod cleanup;
mod edit_state;
mod input_size;
mod misc;
mod types;

pub use cleanup::{
    cleanup_input_state, cleanup_stale_input_states, deferred_input_state_reset, reset_input_state,
};
use edit_state::EditState;
pub use input_size::InputSize;
use misc::keystroke_to_char;
pub use misc::{clear_all_input_states, input_state_count, is_input_editing};
pub use types::{InputTheme, InputVariant};

/// 多行输入框固定行高（逻辑 px），原生编辑器通过 `line_height` 参数对齐。
pub const MULTILINE_LINE_HEIGHT: f32 = 20.0;

/// Everything the platform needs to hand the focused input over to a real
/// native editor (`EditText` on Android, `UITextView` on iOS) overlaid on
/// the input's bounds. The system owns the keyboard, caret, selection
/// handles, and edit menu; edits sync back through the IME bridge.
#[derive(Debug, Clone, PartialEq)]
pub struct TextMenuRequest {
    /// Stable GPUI input id. Native controllers use it to route callbacks back
    /// to the field that initiated the edit.
    pub id: String,
    pub text: String,
    /// `true` when the controlled value changed outside the native editor
    /// (for example, Home's Paste/Clear buttons). Android must apply it even
    /// if focus handoff is still completing.
    pub force_text_sync: bool,
    /// Selection in UTF-16 code units (matching Android/iOS conventions).
    pub selection_utf16: (usize, usize),
    /// Element bounds in window logical px: (x, y, width, height).
    pub bounds: (f32, f32, f32, f32),
    /// Visible window-space clip for transition rendering. The platform
    /// editor remains mounted inside this rectangle while another page slides
    /// above it, so its native window layer cannot cover the incoming page.
    pub clip_bounds: Option<(f32, f32, f32, f32)>,
    /// Text start position in window logical px.
    pub text_origin: (f32, f32),
    /// First-line glyph baseline in window logical px.
    ///
    /// Android positions the native `EditText` so its own baseline lands on
    /// this value. Both sides derive it from live font metrics, so the
    /// preview and the native editor agree at any density or font scale.
    #[cfg(target_os = "android")]
    pub text_baseline: f32,
    /// Approximate text size in logical px.
    pub font_size: f32,
    pub multiline: bool,
    /// GPUI-measured wrap width of the text area in logical px (`0.0` when
    /// unknown). The native editor wraps at exactly this width so focused
    /// and blurred renderings break lines identically.
    pub wrap_width: f32,
    /// GPUI multiline line height in logical px (`0.0` for single-line
    /// inputs); the native editor matches it so row spacing stays equal.
    pub line_height: f32,
    /// GPUI visual line ranges in UTF-16 offsets. The native editor uses
    /// them to constrain its own measured line widths, so Android and GPUI
    /// wrap at exactly the same characters on every device.
    pub line_ranges_utf16: Vec<(usize, usize)>,
    /// `true` when the editor should take focus and raise the keyboard,
    /// `false` for bounds/selection-only updates.
    pub initial: bool,
    /// Colors as `0xRRGGBBAA` so the native editor visually matches GPUI.
    pub text_color: u32,
    pub background_color: u32,
    pub border_color: u32,
    pub focus_border_color: u32,
    pub border_width: f32,
    pub placeholder: Option<String>,
    pub placeholder_color: u32,
    pub corner_radius: f32,
    /// The Rust input's current focus state. The platform-owned field draws
    /// its own focus border from this value on iOS.
    pub focused: bool,
    /// Maximum visible height in logical pixels. The native field uses this
    /// as its own growth ceiling.
    pub max_height: f32,
    /// Base visible height supplied by the screen layout. Native text edits
    /// may grow from here up to `max_height`.
    pub min_height: f32,
}

pub type TextMenuHook = Rc<dyn Fn(&TextMenuRequest)>;
pub type TextMenuHideHook = Rc<dyn Fn()>;
pub type TextMenuHideFieldHook = Rc<dyn Fn(&str)>;
pub type TextMenuTextHook = Rc<dyn Fn() -> Option<String>>;
pub type TextMenuBlurHook = Rc<dyn Fn()>;
pub type NativeEditorLineRangesHook = Rc<dyn Fn() -> Option<(String, Vec<(usize, usize)>)>>;

thread_local! {
    static TEXT_MENU_HOOK: RefCell<Option<TextMenuHook>> = RefCell::new(None);
    static TEXT_MENU_HIDE_HOOK: RefCell<Option<TextMenuHideHook>> = RefCell::new(None);
    static TEXT_MENU_HIDE_FIELD_HOOK: RefCell<Option<TextMenuHideFieldHook>> =
        const { RefCell::new(None) };
    static TEXT_MENU_TEXT_HOOK: RefCell<Option<TextMenuTextHook>> = RefCell::new(None);
    static TEXT_MENU_BLUR_HOOK: RefCell<Option<TextMenuBlurHook>> = RefCell::new(None);
    static NATIVE_EDITOR_LINE_RANGES_HOOK: RefCell<Option<NativeEditorLineRangesHook>> =
        RefCell::new(None);
    static NATIVE_EDITOR_VISIBLE: Cell<bool> = const { Cell::new(false) };
    static NATIVE_EDITOR_FOCUSED: Cell<bool> = const { Cell::new(false) };
    static NATIVE_EDITOR_FOCUS_REPORTED: Cell<bool> = const { Cell::new(false) };
    static ACTIVE_NATIVE_EDITOR_ID: RefCell<Option<ElementId>> = const { RefCell::new(None) };
}

/// Register the platform text-menu bridge (mobile only; desktop is no-op).
pub fn set_text_menu_hooks(
    show: Option<TextMenuHook>,
    hide: Option<TextMenuHideHook>,
    text: Option<TextMenuTextHook>,
) {
    TEXT_MENU_HOOK.with(|cell| *cell.borrow_mut() = show);
    TEXT_MENU_HIDE_HOOK.with(|cell| *cell.borrow_mut() = hide);
    TEXT_MENU_TEXT_HOOK.with(|cell| *cell.borrow_mut() = text);
}

pub fn set_text_menu_hide_field_hook(hook: Option<TextMenuHideFieldHook>) {
    TEXT_MENU_HIDE_FIELD_HOOK.with(|cell| *cell.borrow_mut() = hook);
}

/// Whether a platform native-editor bridge is registered (mobile only).
pub fn has_text_menu_hook() -> bool {
    TEXT_MENU_HOOK.with(|cell| cell.borrow().is_some())
}

pub fn set_native_editor_line_ranges_hook(hook: Option<NativeEditorLineRangesHook>) {
    NATIVE_EDITOR_LINE_RANGES_HOOK.with(|cell| *cell.borrow_mut() = hook);
}

/// Whether the supplied input currently owns the mobile native editor.
///
/// Dialog cleanup must not replace the shared EditText contents or bump its
/// session while another field (notably Home) owns the active IME connection.
pub fn native_editor_owned_by(id: &ElementId) -> bool {
    ACTIVE_NATIVE_EDITOR_ID.with(|active| active.borrow().as_ref() == Some(id))
        && NATIVE_EDITOR_VISIBLE.with(Cell::get)
}

/// Remember text originating in the iOS native editor. The following GPUI
/// render carries the same value through the controlled `Input`; it must not
/// be mistaken for an external Paste/Clear update and echoed back into UIKit.
#[cfg(target_os = "ios")]
pub fn note_native_editor_text_echo(id: &str, text: &str) {
    NATIVE_EDITOR_TEXT_ECHOES.with(|echoes| {
        echoes.borrow_mut().insert(id.to_owned(), text.to_owned());
    });
}

#[cfg(target_os = "ios")]
fn take_native_editor_text_echo(id: &ElementId, text: &str) -> bool {
    let id = id.to_string();
    NATIVE_EDITOR_TEXT_ECHOES.with(|echoes| {
        let mut echoes = echoes.borrow_mut();
        if echoes.get(&id).is_some_and(|echo| echo == text) {
            echoes.remove(&id);
            true
        } else {
            false
        }
    })
}

/// Register the platform blur bridge so a GPUI-side blur ends the native
/// editor's focus/IME session (Android only; desktop is a no-op).
pub fn set_text_menu_blur_hook(hook: Option<TextMenuBlurHook>) {
    TEXT_MENU_BLUR_HOOK.with(|cell| *cell.borrow_mut() = hook);
}

#[cfg(target_os = "android")]
fn dispatch_text_menu_blur() {
    TEXT_MENU_BLUR_HOOK.with(|cell| {
        if let Some(hook) = cell.borrow().as_ref() {
            hook();
        }
    });
}

fn native_editor_line_ranges() -> Option<(String, Vec<(usize, usize)>)> {
    NATIVE_EDITOR_LINE_RANGES_HOOK.with(|cell| cell.borrow().as_ref().and_then(|hook| hook()))
}

/// Number of visual lines the native editor currently reports for `text`.
pub fn native_editor_line_count(text: &str) -> Option<usize> {
    native_editor_line_ranges()
        .filter(|(native_text, _)| native_text == text)
        .map(|(_, ranges)| ranges.len())
}

fn dispatch_text_menu(request: &TextMenuRequest) {
    TEXT_MENU_HOOK.with(|cell| {
        if let Some(hook) = cell.borrow().as_ref() {
            hook(request);
        }
    });
}

/// Hide any platform text menu currently shown for a GPUI input.
pub fn hide_text_menu() {
    let _ = take_native_text_for_commit();
    NATIVE_EDITOR_VISIBLE.with(|visible| visible.set(false));
    NATIVE_EDITOR_FOCUSED.with(|focused| focused.set(false));
    NATIVE_EDITOR_FOCUS_REPORTED.with(|reported| reported.set(false));
    ACTIVE_NATIVE_EDITOR_ID.with(|active| active.borrow_mut().take());
    #[cfg(target_os = "android")]
    NATIVE_EDITOR_SCROLL_HANDLE.with(|handle| {
        *handle.borrow_mut() = None;
    });
    TEXT_MENU_HIDE_HOOK.with(|cell| {
        if let Some(hook) = cell.borrow().as_ref() {
            hook();
        }
    });
}

/// Hide the native editor for one input when a dialog, route transition, or
/// disabled/read-only state makes that field's platform viewport invalid.
pub fn hide_text_menu_for(id: &ElementId) {
    let id = id.to_string();
    TEXT_MENU_HIDE_FIELD_HOOK.with(|cell| {
        if let Some(hook) = cell.borrow().as_ref() {
            hook(&id);
        }
    });
}

#[cfg(target_os = "android")]
pub fn apply_native_editor_scroll(scroll_top: f32) {
    NATIVE_EDITOR_SCROLL_HANDLE.with(|handle| {
        if let Some(handle) = handle.borrow().as_ref() {
            let max_scroll = f32::from(handle.max_offset().y).max(0.0);
            let target = -scroll_top.clamp(0.0, max_scroll);
            if (f32::from(handle.offset().y) - target).abs() > 0.01 {
                handle.set_offset(gpui::point(px(0.0), px(target)));
            }
        }
    });
}

fn native_editor_visible() -> bool {
    NATIVE_EDITOR_VISIBLE.with(Cell::get)
}

/// The platform collapsed the native editor without going through GPUI.
/// Keep the focused input alive, but require a fresh handoff on the next tap.
pub fn mark_native_editor_hidden() {
    NATIVE_EDITOR_VISIBLE.with(|visible| visible.set(false));
    NATIVE_EDITOR_FOCUSED.with(|focused| focused.set(false));
    NATIVE_EDITOR_FOCUS_REPORTED.with(|reported| reported.set(false));
    ACTIVE_NATIVE_EDITOR_ID.with(|active| active.borrow_mut().take());
    #[cfg(target_os = "android")]
    NATIVE_EDITOR_SCROLL_HANDLE.with(|handle| {
        *handle.borrow_mut() = None;
    });
}

/// Platform-native editor gained or lost Android focus. The native view owns
/// focus directly, so mirror it back into GPUI on the next frame.
pub fn handle_native_editor_focus(focused: bool) {
    NATIVE_EDITOR_FOCUSED.with(|current| current.set(focused));
    NATIVE_EDITOR_FOCUS_REPORTED.with(|reported| reported.set(true));
}

#[cfg(target_os = "android")]
fn native_editor_focus_for(id: &ElementId) -> Option<bool> {
    if !NATIVE_EDITOR_FOCUS_REPORTED.with(Cell::get) {
        return None;
    }
    ACTIVE_NATIVE_EDITOR_ID.with(|active| {
        active
            .borrow()
            .as_ref()
            .filter(|active_id| *active_id == id)
            .map(|_| NATIVE_EDITOR_FOCUSED.with(Cell::get))
    })
}

fn take_native_text_for_commit() -> Option<String> {
    TEXT_MENU_TEXT_HOOK.with(|cell| cell.borrow().as_ref().and_then(|hook| hook()))
}

/// The current native text selection, expressed in Unicode scalar indices.
///
/// Callers receive only positions, never the selected text. This makes the
/// callback safe to use with password inputs and protocol traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSelection {
    pub start: usize,
    pub end: usize,
    pub reversed: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum MultilineClickTarget {
    Layout,
    Char(usize),
}

fn multiline_click_target(
    text: &str,
    clicked_row: usize,
    content_last_row: usize,
    max_clickable_row: usize,
) -> MultilineClickTarget {
    let content = text.trim_end_matches('\n');
    let trailing_newlines = text.len() - content.len();
    let content_chars = content.chars().count();
    let last_row = content_last_row + trailing_newlines;
    let target_row = clicked_row.min(last_row.max(max_clickable_row));

    if target_row > last_row {
        // Never inject synthetic newlines: tapping blank space below the text
        // just moves the caret to the end, like native text fields do.
        MultilineClickTarget::Char(content_chars + trailing_newlines)
    } else if target_row > content_last_row {
        MultilineClickTarget::Char(content_chars + (target_row - content_last_row))
    } else {
        MultilineClickTarget::Layout
    }
}

/// Schema-style validator for [`Input`]: maps a value to an error message.
///
/// `None` means the value is valid. Validators are synchronous; report async
/// validation progress with an explicit [`Input::error`] message instead.
pub type InputValidator = Rc<dyn Fn(&str) -> Option<SharedString> + 'static>;

/// A text input component with full keyboard editing support
///
/// The Input handles all focus and keyboard events internally.
/// Parent components only need to provide callbacks for value changes.
#[derive(gpui::IntoElement)]
pub struct Input {
    multiline: bool,
    min_lines: usize,
    id: ElementId,
    value: SharedString,
    placeholder: Option<SharedString>,
    label: Option<SharedString>,
    size: InputSize,
    horizontal_padding: Option<gpui::Rems>,
    variant: InputVariant,
    disabled: bool,
    readonly: bool,
    password: bool,
    select_all_on_ctrl: bool,
    error: Option<SharedString>,
    /// Schema-style validator: maps a value to an error message (`None` = valid).
    validator: Option<InputValidator>,
    icon_left: Option<SharedString>,
    icon_right: Option<SharedString>,
    bg_color: Option<Rgba>,
    text_color: Option<Rgba>,
    border_color: Option<Rgba>,
    placeholder_color: Option<Rgba>,
    /// Maximum visible height for the platform-owned editor viewport.
    editor_viewport_height: Option<f32>,
    /// Minimum visible height for the native editor while text is short.
    native_editor_min_height: Option<f32>,
    /// Outer GPUI scroll container that owns the visible multiline viewport.
    editor_scroll_handle: Option<gpui::ScrollHandle>,
    /// Android renders the editable surface with a native EditText. Hiding
    /// this while a route transition is running prevents the native view from
    /// hovering above the outgoing/incoming GPUI page.
    native_editor_enabled: bool,
    /// Optional window-space clipping rectangle for the native editor layer.
    /// This keeps page transitions layered correctly without remounting the
    /// editor and changing its text wrapping.
    native_editor_clip_bounds: Option<(f32, f32, f32, f32)>,
    /// Called when value is confirmed (Enter pressed)
    on_change: Option<Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    /// Called when editing starts (click on input)
    on_edit_start: Option<Rc<dyn Fn(&mut Window, &mut App) + 'static>>,
    /// Called when editing ends (Enter = Some(value), Escape = None)
    on_edit_end: Option<Rc<dyn Fn(Option<String>, &mut Window, &mut App) + 'static>>,
    /// Called on every text change during editing (for live updates)
    on_text_change: Option<Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    /// Called when the cursor or selected range changes. The callback exposes
    /// positions only, never the selected value.
    on_selection_change: Option<Rc<dyn Fn(InputSelection, &mut Window, &mut App) + 'static>>,
    /// Focus handle for this input
    focus_handle: Option<FocusHandle>,
    aria_label: Option<SharedString>,
    aria_role: Option<AriaRole>,
}

impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Input");
        debug
            .field("id", &self.id)
            .field("placeholder", &self.placeholder)
            .field("label", &self.label)
            .field("size", &self.size)
            .field("variant", &self.variant)
            .field("disabled", &self.disabled)
            .field("readonly", &self.readonly)
            .field("password", &self.password);
        if self.password {
            debug
                .field("value", &"<redacted>")
                .field("value_char_count", &self.value.chars().count());
        } else {
            debug.field("value", &self.value);
        }
        debug.finish_non_exhaustive()
    }
}

impl Input {
    /// Create a new input
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            multiline: false,
            min_lines: 2,
            id: id.into(),
            value: "".into(),
            placeholder: None,
            label: None,
            size: InputSize::default(),
            horizontal_padding: None,
            variant: InputVariant::default(),
            disabled: false,
            readonly: false,
            password: false,
            select_all_on_ctrl: false,
            error: None,
            validator: None,
            icon_left: None,
            icon_right: None,
            bg_color: None,
            text_color: None,
            border_color: None,
            placeholder_color: None,
            editor_viewport_height: None,
            native_editor_min_height: None,
            editor_scroll_handle: None,
            native_editor_enabled: true,
            native_editor_clip_bounds: None,
            on_change: None,
            on_edit_start: None,
            on_edit_end: None,
            on_text_change: None,
            on_selection_change: None,
            focus_handle: None,
            aria_label: None,
            aria_role: None,
        }
    }

    /// Enables wrapping multiline composition with Enter submit and Shift-Enter newline.
    ///
    /// In this mode `on_change` runs only on Enter, never on focus loss.
    pub fn multiline(mut self, enabled: bool) -> Self {
        self.multiline = enabled;
        self
    }

    /// Set the minimum visible line count for multiline inputs.
    pub fn min_lines(mut self, lines: usize) -> Self {
        self.min_lines = lines.max(1);
        self
    }

    /// Limit the native editor to the visible multiline viewport.
    ///
    /// GPUI can lay out the full text height inside an outer scroll container,
    /// but the platform editor must keep the scrollable viewport height so
    /// dragging scrolls its own content and caret/selection stay visible.
    pub fn editor_viewport_height(mut self, height: f32) -> Self {
        self.editor_viewport_height = Some(height.max(1.0));
        self
    }

    /// Minimum visible height for a platform-owned multiline editor.
    pub fn native_editor_min_height(mut self, height: f32) -> Self {
        self.native_editor_min_height = Some(height.max(1.0));
        self
    }

    /// Keep the visible GPUI scroll container in sync with the native editor
    /// so keyboard/gesture scrolling, caret movement, and selection handles
    /// all reveal the same viewport.
    pub fn editor_scroll_handle(mut self, handle: gpui::ScrollHandle) -> Self {
        self.editor_scroll_handle = Some(handle);
        self
    }

    /// Allow Android to keep a native editor bound to this field.
    pub fn native_editor_enabled(mut self, enabled: bool) -> Self {
        self.native_editor_enabled = enabled;
        self
    }

    /// Clip the platform-owned editor to the part of the window still owned
    /// by its GPUI page during a transition.
    pub fn native_editor_clip_bounds(mut self, bounds: Option<(f32, f32, f32, f32)>) -> Self {
        self.native_editor_clip_bounds = bounds;
        self
    }

    /// Use Control+A to select all text.
    ///
    /// Disabled by default to preserve Emacs line navigation. Browser apps
    /// can enable this for Windows and Linux while keeping macOS bindings.
    pub fn select_all_on_ctrl(mut self, enabled: bool) -> Self {
        self.select_all_on_ctrl = enabled;
        self
    }

    /// Set the focus handle (optional - one is created internally if not provided)
    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.focus_handle = Some(handle);
        self
    }

    /// Set the input value
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set label text
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set input size
    pub fn size(mut self, size: InputSize) -> Self {
        self.size = size;
        self
    }

    /// Override horizontal padding for compact embedded editors.
    pub fn horizontal_padding(mut self, padding: gpui::Rems) -> Self {
        self.horizontal_padding = Some(padding);
        self
    }

    /// Set input variant
    pub fn variant(mut self, variant: InputVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set readonly state
    pub fn readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }

    /// Mask the rendered value while preserving normal text editing and change
    /// callbacks. The native accessibility value is masked as well.
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    /// Set error message
    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set a schema-style validator run against the value on every render.
    ///
    /// The validator maps the current value to an error message (`None` means
    /// valid). An explicit [`Self::error`] message takes precedence over the
    /// validator output. Async validation is out of scope: return a pending
    /// message synchronously while the async check runs elsewhere.
    ///
    /// # Example
    ///
    /// ```ignore
    /// Input::new("email")
    ///     .validator(|value| {
    ///         if value.contains('@') {
    ///             None
    ///         } else {
    ///             Some("Enter a valid email address".into())
    ///         }
    ///     })
    /// ```
    pub fn validator(mut self, validator: impl Fn(&str) -> Option<SharedString> + 'static) -> Self {
        self.validator = Some(Rc::new(validator));
        self
    }

    /// Run the validator against `value`, returning the error message if invalid.
    ///
    /// Returns `None` when no validator is set or the value passes. Use
    /// [`Self::effective_error`] to include an explicit [`Self::error`] message.
    pub fn validate(&self, value: &str) -> Option<SharedString> {
        self.validator
            .as_ref()
            .and_then(|validator| validator(value))
    }

    /// Explicit [`Self::error`] message if set, otherwise the validator output
    /// for the current value.
    pub fn effective_error(&self) -> Option<SharedString> {
        self.error
            .clone()
            .or_else(|| self.validate(self.value.as_ref()))
    }

    /// Set left icon
    pub fn icon_left(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon_left = Some(icon.into());
        self
    }

    /// Set right icon
    pub fn icon_right(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon_right = Some(icon.into());
        self
    }

    /// Set background color
    pub fn bg_color(mut self, color: impl Into<Rgba>) -> Self {
        self.bg_color = Some(color.into());
        self
    }

    /// Set text color
    pub fn text_color(mut self, color: impl Into<Rgba>) -> Self {
        self.text_color = Some(color.into());
        self
    }

    /// Set border color
    pub fn border_color(mut self, color: impl Into<Rgba>) -> Self {
        self.border_color = Some(color.into());
        self
    }

    /// Set placeholder color
    pub fn placeholder_color(mut self, color: impl Into<Rgba>) -> Self {
        self.placeholder_color = Some(color.into());
        self
    }

    /// Set change handler (called when input value is confirmed with Enter)
    pub fn on_change(mut self, handler: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    /// Set edit start handler (called when user clicks on input to edit)
    pub fn on_edit_start(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_edit_start = Some(Rc::new(handler));
        self
    }

    /// Set edit end handler (called when user confirms or cancels edit)
    /// The `Option<String>` is `Some(value)` if confirmed, `None` if cancelled
    pub fn on_edit_end(
        mut self,
        handler: impl Fn(Option<String>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_edit_end = Some(Rc::new(handler));
        self
    }

    /// Set text change handler (called on every keystroke during editing)
    pub fn on_text_change(
        mut self,
        handler: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_text_change = Some(Rc::new(handler));
        self
    }

    /// Set a selection-change handler. Positions use Unicode scalar indices
    /// and are suitable for serializing into application events.
    pub fn on_selection_change(
        mut self,
        handler: impl Fn(InputSelection, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_selection_change = Some(Rc::new(handler));
        self
    }

    /// Set an explicit ARIA label
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    /// Override the default ARIA role (Textbox)
    pub fn aria_role(mut self, role: AriaRole) -> Self {
        self.aria_role = Some(role);
        self
    }
}

impl InputEntity {
    fn cached_password_mask(
        cache: &RefCell<Option<(usize, SharedString)>>,
        source: &str,
    ) -> SharedString {
        let char_count = source.chars().count();
        if let Some((cached_char_count, mask)) = cache.borrow().as_ref()
            && *cached_char_count == char_count
        {
            return mask.clone();
        }
        let mask: SharedString = "•".repeat(char_count).into();
        // Deliberately retain only the public character count and mask. A
        // password must not be duplicated into a render/accessibility cache.
        *cache.borrow_mut() = Some((char_count, mask.clone()));
        mask
    }

    fn char_to_utf16(text: &str, char_idx: usize) -> usize {
        text.chars().take(char_idx).map(char::len_utf16).sum()
    }

    fn utf16_to_char(text: &str, utf16_idx: usize) -> usize {
        let mut utf16_pos = 0;
        for (char_idx, ch) in text.chars().enumerate() {
            let next = utf16_pos + ch.len_utf16();
            if utf16_idx < next {
                return char_idx;
            }
            utf16_pos = next;
        }
        text.chars().count()
    }

    /// Return the two UTF-8 byte boundaries in one character scan. Editing
    /// rendering needs several character-indexed pieces per frame; deriving
    /// both boundaries together avoids the old repeated O(n) `nth` walks.
    fn char_range_byte_offsets(text: &str, start: usize, end: usize) -> (usize, usize) {
        let mut start_byte = text.len();
        let mut end_byte = text.len();
        for (char_index, (byte_index, _)) in text.char_indices().enumerate() {
            if char_index == start {
                start_byte = byte_index;
            }
            if char_index == end {
                end_byte = byte_index;
                break;
            }
        }
        (start_byte, end_byte)
    }

    fn replace_char_range(state: &mut EditState, range: Range<usize>, text: &str) {
        state.begin_text_edit();
        let char_len = state.text.chars().count();
        let start = range.start.min(char_len);
        let end = range.end.min(char_len).max(start);
        let start_byte = state
            .text
            .char_indices()
            .nth(start)
            .map_or(state.text.len(), |(idx, _)| idx);
        let end_byte = state
            .text
            .char_indices()
            .nth(end)
            .map_or(state.text.len(), |(idx, _)| idx);

        state.text.replace_range(start_byte..end_byte, text);
        state.cursor = start + text.chars().count();
        state.clear_selection();
    }

    fn current_selected_char_range(state: &EditState) -> Range<usize> {
        if let Some((start, end)) = state.selection_range() {
            start..end
        } else {
            state.cursor..state.cursor
        }
    }

    fn replacement_char_range(
        state: &EditState,
        range_utf16: Option<Range<usize>>,
        marked_range_utf16: Option<Range<usize>>,
    ) -> Range<usize> {
        range_utf16.or(marked_range_utf16).map_or_else(
            || Self::current_selected_char_range(state),
            |range| {
                Self::utf16_to_char(&state.text, range.start)
                    ..Self::utf16_to_char(&state.text, range.end)
            },
        )
    }

    fn ensure_editing_state(&mut self) {
        let mut state = self.edit_state.borrow_mut();
        if !state.editing {
            *state = EditState::new(self.props.value.as_ref());
            state.clear_selection();
        }
    }

    fn emit_text_change(&self, text: &str, window: &mut Window, cx: &mut App) {
        #[cfg(target_os = "android")]
        if self.props.on_text_change.is_some() {
            *self.last_local_text_update.borrow_mut() = Some(text.to_owned());
        }
        if let Some(ref handler) = self.props.on_text_change {
            handler(text, window, cx);
        }
    }

    fn selection_from_state(state: &EditState) -> InputSelection {
        let range = Self::current_selected_char_range(state);
        InputSelection {
            start: range.start,
            end: range.end,
            reversed: state
                .selection_anchor
                .is_some_and(|anchor| state.cursor < anchor),
        }
    }

    fn emit_selection_change(&self, selection: InputSelection, window: &mut Window, cx: &mut App) {
        if let Some(ref handler) = self.props.on_selection_change {
            handler(selection, window, cx);
        }
    }

    /// Keep the caret inside the parent multiline scroll viewport.
    fn reveal_cursor(&self) {
        if !self.props.multiline || cfg!(target_os = "android") {
            return;
        }
        let Some(handle) = self.props.editor_scroll_handle.as_ref() else {
            return;
        };
        let Some(layout) = self.multiline_layout.as_ref() else {
            return;
        };
        let state = self.edit_state.borrow();
        let visual_map = self.multiline_visual_map.borrow();
        let byte = visual_map.as_ref().map_or_else(
            || Self::char_range_byte_offsets(&state.text, state.cursor, state.cursor).0,
            |map| {
                Self::char_range_byte_offsets(
                    &map.display_text,
                    map.cursor_char(state.cursor),
                    map.cursor_char(state.cursor),
                )
                .0
            },
        );
        let Some(position) = layout.position_for_index(byte) else {
            return;
        };
        let viewport = handle.bounds();
        if f32::from(viewport.size.height) <= 0.0 {
            return;
        }
        let line_height = f32::from(layout.line_height()).max(1.0);
        let caret_top = f32::from(position.y);
        let caret_bottom = caret_top + line_height;
        let viewport_top = f32::from(viewport.origin.y);
        let viewport_bottom = viewport_top + f32::from(viewport.size.height);
        let max_scroll = f32::from(handle.max_offset().y).max(0.0);
        let scroll_top = (-f32::from(handle.offset().y)).clamp(0.0, max_scroll);
        let target = if caret_top < viewport_top {
            (scroll_top + caret_top - viewport_top).max(0.0)
        } else if caret_bottom > viewport_bottom {
            (scroll_top + caret_bottom - viewport_bottom).min(max_scroll)
        } else {
            return;
        };
        if (target - scroll_top).abs() > 0.5 {
            handle.set_offset(gpui::point(px(0.0), px(-target)));
        }
    }

    /// Continue a mouse selection while the pointer is held at a viewport edge.
    fn reveal_pointer_edge(&self, pointer: gpui::Point<Pixels>) -> bool {
        if !self.props.multiline || cfg!(target_os = "android") {
            return false;
        }
        let Some(handle) = self.props.editor_scroll_handle.as_ref() else {
            return false;
        };
        let viewport = handle.bounds();
        let height = f32::from(viewport.size.height);
        if height <= 0.0 {
            return false;
        }
        let y = f32::from(pointer.y);
        let top = f32::from(viewport.origin.y);
        let bottom = top + height;
        let edge = (height * 0.2).min(24.0).max(8.0);
        let step = MULTILINE_LINE_HEIGHT.max(1.0);
        let delta = if y < top + edge {
            -step
        } else if y > bottom - edge {
            step
        } else {
            return false;
        };
        let max_scroll = f32::from(handle.max_offset().y).max(0.0);
        let current = -f32::from(handle.offset().y);
        let target = (current + delta).clamp(0.0, max_scroll);
        if (target - current).abs() <= 0.5 {
            return false;
        }
        handle.set_offset(gpui::point(px(0.0), px(-target)));
        true
    }

    /// Reveal the caret after the new text has been measured and laid out.
    fn reveal_cursor_next_frame(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.props.multiline || cfg!(target_os = "android") {
            return;
        }
        cx.on_next_frame(window, |model, _, _| model.reveal_cursor());
    }

    /// Keep scrolling a selection while the pointer stays at a viewport edge.
    fn continue_edge_selection(
        &self,
        pointer: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.props.multiline
            || cfg!(target_os = "android")
            || !self.edit_state.borrow().is_dragging
        {
            return;
        }
        if !self.reveal_pointer_edge(pointer) {
            return;
        }
        let mut state = self.edit_state.borrow_mut();
        let visual_map = self.multiline_visual_map.borrow();
        if let Some(layout) = self.multiline_layout.as_ref() {
            let byte = layout
                .index_for_position(pointer)
                .unwrap_or_else(|i| i)
                .min(
                    visual_map
                        .as_ref()
                        .map_or(state.text.len(), |map| map.display_text.len()),
                );
            let char_pos = visual_map.as_ref().map_or_else(
                || state.text[..byte].chars().count(),
                |map| Self::original_char_for_display_byte(map, byte),
            );
            state.update_selection(char_pos);
        }
        let selection = Self::selection_from_state(&state);
        drop(state);
        self.emit_selection_change(selection, window, cx);
        self.continue_edge_selection_next_frame(pointer, window, cx);
        window.refresh();
    }

    fn continue_edge_selection_next_frame(
        &self,
        pointer: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.on_next_frame(window, move |model, window, cx| {
            model.continue_edge_selection(pointer, window, cx);
        });
    }

    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let native_text = take_native_text_for_commit();
        #[cfg(target_os = "android")]
        if has_text_menu_hook() {
            // Android keeps the native EditText mounted as the visible input.
            // Finishing the GPUI edit state here would clear the draft that
            // the native editor still owns. Commit the latest snapshot and
            // keep the editing state alive until the route/editor is dismissed.
            let mut state = self.edit_state.borrow_mut();
            if let Some(native_text) = native_text.clone() {
                state.text = native_text;
            } else if !state.editing {
                state.text = self.props.value.to_string();
            }
            let text = if state.editing {
                state.text.clone()
            } else {
                self.props.value.to_string()
            };
            state.text = text.clone();
            state.editing = true;
            drop(state);

            if self.props.value.as_ref() != text {
                self.props.value = text.clone().into();
                if let Some(ref handler) = self.props.on_change {
                    handler(&text, window, cx);
                }
                self.emit_text_change(&text, window, cx);
            }
            window.refresh();
            return;
        }

        NATIVE_EDITOR_VISIBLE.with(|visible| visible.set(false));
        TEXT_MENU_HIDE_HOOK.with(|cell| {
            if let Some(hook) = cell.borrow().as_ref() {
                hook();
            }
        });
        self.marked_text_range.borrow_mut().take();

        let mut state = self.edit_state.borrow_mut();
        if !state.editing {
            return;
        }

        if let Some(native_text) = native_text {
            state.text = native_text;
        }
        let text = state.finish_edit();
        drop(state);
        // Keep the widget's immediately available value in sync with the
        // commit. The parent rerender may not run before the user focuses
        // again; seeding the next editing session from a stale prop would
        // make the freshly cleared native editor take over with blank text.
        self.props.value = text.clone().into();

        if let Some(ref handler) = self.props.on_change {
            handler(&text, window, cx);
        }
        self.emit_text_change(&text, window, cx);
        if let Some(ref handler) = self.props.on_edit_end {
            handler(Some(text), window, cx);
        }
        window.refresh();
    }

    /// Show (or refresh) the real native editor that takes over the
    /// focused input on mobile. Desktop is a no-op (no hook registered).
    fn show_native_editor(&self, initial: bool, cx: &App) {
        if let Some(mut request) = self.build_native_editor_request(cx, initial) {
            let owns_existing_session = native_editor_visible()
                && ACTIVE_NATIVE_EDITOR_ID
                    .with(|active| active.borrow().as_ref() == Some(&self.props.id));
            if request.initial && (!owns_existing_session || !NATIVE_EDITOR_FOCUSED.with(Cell::get))
            {
                NATIVE_EDITOR_FOCUSED.with(|focused| focused.set(false));
                NATIVE_EDITOR_FOCUS_REPORTED.with(|reported| reported.set(false));
            }
            ACTIVE_NATIVE_EDITOR_ID.with(|active| {
                *active.borrow_mut() = Some(self.props.id.clone());
            });
            let unchanged = !request.initial
                && native_editor_visible()
                && self
                    .last_native_editor_request
                    .borrow()
                    .as_ref()
                    .is_some_and(|previous| previous == &request);
            if unchanged {
                return;
            }
            dispatch_text_menu(&request);
            self.native_editor_force_text_sync.set(false);
            request.initial = false;
            *self.last_native_editor_request.borrow_mut() = Some(request);
            NATIVE_EDITOR_VISIBLE.with(|visible| visible.set(true));
        }
    }

    /// Assemble the native-editor handoff payload from the current editing
    /// state and the last painted geometry. Returns `None` while the input
    /// has never been painted or is not editable.
    fn build_native_editor_request(&self, cx: &App, initial: bool) -> Option<TextMenuRequest> {
        let mut element_bounds = (*self.element_bounds.borrow())?;
        let font_size = self.paint_font_size.get();
        let state = self.edit_state.borrow();
        let text = if state.editing {
            state.text.clone()
        } else {
            self.props.value.to_string()
        };
        let selection_utf16 = if state.editing {
            let (start_char, end_char) = state
                .selection_range()
                .unwrap_or((state.cursor, state.cursor));
            (
                Self::char_to_utf16(&text, start_char),
                Self::char_to_utf16(&text, end_char),
            )
        } else {
            let end = text.encode_utf16().count();
            (end, end)
        };
        let multiline = self.props.multiline;
        let wrap_width = if multiline {
            self.text_area_bounds
                .borrow()
                .map(|(_, _, width, _)| width)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let line_height = if multiline {
            MULTILINE_LINE_HEIGHT
        } else {
            0.0
        };
        let line_ranges_utf16 = if multiline {
            self.multiline_layout
                .as_ref()
                .map(|layout| Self::visual_line_ranges_utf16(layout, &text))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut text_origin = if multiline {
            self.multiline_layout
                .as_ref()
                .and_then(|layout| layout.position_for_index(0))
                .map(|position| {
                    let x: f32 = position.x.into();
                    let y: f32 = position.y.into();
                    (x, y)
                })
                .unwrap_or((element_bounds.0, element_bounds.1))
        } else {
            let origin_x = TEXT_ORIGINS
                .with(|o| o.borrow().get(&self.props.id).copied())
                .unwrap_or(element_bounds.0);
            (
                origin_x,
                element_bounds.1 + (element_bounds.3 - font_size) * 0.5,
            )
        };
        if multiline {
            let viewport = self
                .props
                .editor_scroll_handle
                .as_ref()
                .map(gpui::ScrollHandle::bounds);
            if let Some(viewport) = viewport
                && viewport.size.width > px(0.0)
                && viewport.size.height > px(0.0)
            {
                let viewport_x: f32 = viewport.origin.x.into();
                let viewport_y: f32 = viewport.origin.y.into();
                let viewport_width: f32 = viewport.size.width.into();
                let viewport_height: f32 = viewport.size.height.into();
                let text_offset_x = text_origin.0 - element_bounds.0;
                let text_offset_y = text_origin.1 - element_bounds.1;
                let height = self
                    .props
                    .editor_viewport_height
                    .map_or(viewport_height, |max_height| {
                        viewport_height.min(max_height)
                    });
                element_bounds = (viewport_x, viewport_y, viewport_width, height);
                text_origin = (viewport_x + text_offset_x, viewport_y + text_offset_y);
            } else if let Some(max_height) = self.props.editor_viewport_height {
                element_bounds.3 = element_bounds.3.min(max_height);
            }
        }
        // GPUI places a line's glyphs on `padding_top + ascent`, where the
        // padding centres the font's ascent+descent box inside the line box.
        // Report that baseline so the native editor can put its own glyphs on
        // the very same pixel row instead of relying on a fixed offset.
        #[cfg(target_os = "android")]
        let text_baseline = if multiline {
            self.multiline_layout
                .as_ref()
                .and_then(|layout| {
                    let line = layout.line_layout_for_index(0)?;
                    let line_height = f32::from(layout.line_height());
                    let ascent = f32::from(line.ascent());
                    let descent = f32::from(line.descent());
                    Some(text_origin.1 + (line_height - ascent - descent) * 0.5 + ascent)
                })
                .unwrap_or(text_origin.1)
        } else {
            text_origin.1
        };
        let global_theme = cx
            .try_global::<ThemeState>()
            .map(|state| state.theme.clone());
        let theme = InputTheme::from_theme(global_theme.as_deref().unwrap_or(&Theme::default()));
        let custom_background = self.props.bg_color;
        let custom_border = self.props.border_color;
        let background_color = match self.props.variant {
            InputVariant::Default => self.props.bg_color.unwrap_or(theme.background),
            InputVariant::Filled => self.props.bg_color.unwrap_or(theme.filled_bg),
            InputVariant::Flushed => theme.transparent,
        };
        Some(TextMenuRequest {
            id: self.props.id.to_string(),
            text,
            force_text_sync: self.native_editor_force_text_sync.get(),
            selection_utf16,
            bounds: element_bounds,
            clip_bounds: self.props.native_editor_clip_bounds,
            text_origin,
            #[cfg(target_os = "android")]
            text_baseline,
            font_size,
            multiline,
            wrap_width,
            line_height,
            line_ranges_utf16,
            initial,
            text_color: u32::from(self.props.text_color.unwrap_or(theme.text)),
            background_color: u32::from(custom_background.unwrap_or(background_color)),
            border_color: u32::from(
                custom_border
                    .filter(|color| color.a > 0.0)
                    .unwrap_or(theme.border),
            ),
            focus_border_color: u32::from(theme.border_focus),
            border_width: 1.0,
            placeholder: self
                .props
                .placeholder
                .as_ref()
                .map(|placeholder| placeholder.to_string()),
            placeholder_color: u32::from(self.props.placeholder_color.unwrap_or(theme.placeholder)),
            corner_radius: 6.0,
            focused: self.render_focused.get(),
            max_height: self
                .props
                .editor_viewport_height
                .unwrap_or(element_bounds.3),
            min_height: self
                .props
                .native_editor_min_height
                .map(|height| height.min(self.props.editor_viewport_height.unwrap_or(height)))
                .unwrap_or_else(|| {
                    element_bounds.3.min(
                        self.props
                            .editor_viewport_height
                            .unwrap_or(element_bounds.3),
                    )
                }),
        })
    }

    fn multiline_visual_map(&self, text: &str) -> MultilineVisualMap {
        native_editor_line_ranges()
            .filter(|(native_text, _)| native_text == text)
            .map(|(_, ranges)| MultilineVisualMap::new(text, &ranges))
            .unwrap_or_else(|| MultilineVisualMap::new(text, &[]))
    }

    fn display_byte_for_original(
        map: &MultilineVisualMap,
        original_char: usize,
        line_end: bool,
    ) -> usize {
        let display_char = if line_end {
            map.end_char(original_char)
        } else {
            map.cursor_char(original_char)
        };
        Self::char_range_byte_offsets(&map.display_text, display_char, display_char).0
    }

    fn original_char_for_display_byte(map: &MultilineVisualMap, display_byte: usize) -> usize {
        let mut byte = display_byte.min(map.display_text.len());
        while byte > 0 && !map.display_text.is_char_boundary(byte) {
            byte -= 1;
        }
        map.original_char(map.display_text[..byte].chars().count())
    }

    fn visual_line_ranges_utf16(layout: &gpui::TextLayout, text: &str) -> Vec<(usize, usize)> {
        let mut ranges = Vec::new();
        let mut paragraph_start_byte = 0usize;
        let byte_to_utf16 = |byte: usize| {
            let mut byte = byte.min(text.len());
            while byte > 0 && !text.is_char_boundary(byte) {
                byte -= 1;
            }
            text[..byte].encode_utf16().count()
        };

        for line in layout.line_layouts() {
            let mut part_start_byte = 0usize;
            for boundary in line.wrap_boundaries() {
                let Some(run) = line.runs().get(boundary.run_ix) else {
                    continue;
                };
                let Some(glyph) = run.glyphs.get(boundary.glyph_ix) else {
                    continue;
                };
                let part_end_byte = glyph.index.min(line.len());
                ranges.push((
                    byte_to_utf16(paragraph_start_byte + part_start_byte),
                    byte_to_utf16(paragraph_start_byte + part_end_byte),
                ));
                part_start_byte = part_end_byte;
            }
            ranges.push((
                byte_to_utf16(paragraph_start_byte + part_start_byte),
                byte_to_utf16(paragraph_start_byte + line.len()),
            ));
            paragraph_start_byte += line.len() + 1;
        }

        ranges
    }

    fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked_text_range.borrow_mut().take();
        // Focus the input
        window.focus(&self.focus_handle, cx);

        let mut state = self.edit_state.borrow_mut();

        // Keep the synchronized edit buffer after the platform hides its
        // native editor (IME dismissal, app backgrounding, etc.). Resetting
        // from `props.value` here loses large drafts that were mirrored into
        // GPUI but have not completed a parent commit yet. The native editor
        // is still seeded again from this buffer below.
        let resume_existing_edit = state.editing;
        if !resume_existing_edit {
            *state = EditState::new(self.props.value.as_ref());
        }

        // Calculate cursor position from click.
        let edit_text = state.text.clone();
        let text_len = edit_text.chars().count();
        let char_width = 8.0_f32;
        let click_x: f32 = event.position.x.into();
        let id = self.props.id.clone();

        let stored_origin = TEXT_ORIGINS.with(|o| o.borrow().get(&id).copied());
        let char_pos_f = click_x / char_width;
        let origin = stored_origin.unwrap_or_else(|| {
            let cp = char_pos_f.round().min(text_len as f32);
            (click_x - cp * char_width).max(0.0)
        });
        TEXT_ORIGINS.with(|o| {
            o.borrow_mut().insert(id, origin);
        });

        let char_pos = if self.props.multiline {
            let visual_map = self.multiline_visual_map.borrow();
            self.multiline_layout.as_ref().map_or(text_len, |layout| {
                let Some(text_top) = layout.position_for_index(0).map(|position| position.y) else {
                    return text_len;
                };
                let line_height = layout.line_height();
                let row_for_y =
                    |y: Pixels| ((y - text_top) / line_height).round().max(0.0) as usize;
                let clicked_row = ((event.position.y - text_top) / line_height)
                    .floor()
                    .max(0.0) as usize;
                let max_clickable_row = self.props.min_lines.saturating_sub(1);

                if edit_text.is_empty() {
                    return 0;
                }

                let content = edit_text.trim_end_matches('\n');
                let content_char_len = content.chars().count();
                let content_display_char = visual_map
                    .as_ref()
                    .map_or(content_char_len, |map| map.end_char(content_char_len));
                let content_display_byte = visual_map.as_ref().map_or_else(
                    || content.len(),
                    |map| {
                        Self::char_range_byte_offsets(
                            &map.display_text,
                            content_display_char,
                            content_display_char,
                        )
                        .0
                    },
                );
                let content_last_row = if content.is_empty() {
                    Some(0)
                } else {
                    layout
                        .position_for_index(content_display_byte)
                        .or_else(|| {
                            content_display_byte
                                .checked_sub(1)
                                .map(|byte| {
                                    let mut byte = byte;
                                    while byte > 0
                                        && visual_map.as_ref().is_some_and(|map| {
                                            !map.display_text.is_char_boundary(byte)
                                        })
                                    {
                                        byte -= 1;
                                    }
                                    byte
                                })
                                .and_then(|byte| layout.position_for_index(byte))
                        })
                        .map(|position| row_for_y(position.y))
                };
                let Some(content_last_row) = content_last_row else {
                    return text_len;
                };

                match multiline_click_target(
                    &edit_text,
                    clicked_row,
                    content_last_row,
                    max_clickable_row,
                ) {
                    MultilineClickTarget::Char(position) => position,
                    MultilineClickTarget::Layout if content.is_empty() => 0,
                    MultilineClickTarget::Layout => {
                        let probe_y = text_top + line_height * (clicked_row as f32 + 0.5);
                        let byte = layout
                            .index_for_position(gpui::point(event.position.x, probe_y))
                            .unwrap_or_else(|i| i)
                            .min(
                                visual_map
                                    .as_ref()
                                    .map_or(edit_text.len(), |map| map.display_text.len()),
                            );
                        visual_map.as_ref().map_or_else(
                            || edit_text[..byte].chars().count(),
                            |map| Self::original_char_for_display_byte(map, byte),
                        )
                    }
                }
            })
        } else {
            (((click_x - origin) / char_width).round() as usize).min(text_len)
        };

        let was_editing = resume_existing_edit;
        state.editing = true;
        if event.click_count == 2 {
            // Double-click selects the word under the cursor (native text
            // field behaviour) instead of the whole input.
            state.select_word_at(char_pos);
        } else {
            state.start_selection(char_pos);
        }
        let selection = Self::selection_from_state(&state);
        drop(state);

        if !was_editing && let Some(ref handler) = self.props.on_edit_start {
            handler(window, cx);
        }
        self.emit_selection_change(selection, window, cx);
        window.refresh();
        // Hand the focused input over to the platform's real editor; the
        // system then owns the keyboard, caret, selection handles, and its
        // own edit menu. No selection is pre-seeded: the user long-presses
        // or double-taps the editor itself for native word selection.
        #[cfg(target_os = "ios")]
        let initial_native_handoff = true;
        #[cfg(not(target_os = "ios"))]
        let initial_native_handoff = {
            let needs_fresh_native_handoff = has_text_menu_hook() && !native_editor_visible();
            !was_editing || needs_fresh_native_handoff || cfg!(target_os = "android")
        };
        self.show_native_editor(initial_native_handoff, cx);
    }

    fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut state = self.edit_state.borrow_mut();
        if state.is_dragging && state.editing {
            let edit_text = state.text.clone();
            let text_len = edit_text.chars().count();
            let char_width = 8.0_f32;
            let move_x: f32 = event.position.x.into();
            let id = self.props.id.clone();
            let origin = TEXT_ORIGINS.with(|o| o.borrow().get(&id).copied().unwrap_or(0.0));
            let char_pos = if self.props.multiline {
                let visual_map = self.multiline_visual_map.borrow();
                self.multiline_layout.as_ref().map_or(text_len, |layout| {
                    let byte = layout
                        .index_for_position(event.position)
                        .unwrap_or_else(|i| i)
                        .min(
                            visual_map
                                .as_ref()
                                .map_or(edit_text.len(), |map| map.display_text.len()),
                        );
                    visual_map.as_ref().map_or_else(
                        || edit_text[..byte].chars().count(),
                        |map| Self::original_char_for_display_byte(map, byte),
                    )
                })
            } else {
                (((move_x - origin) / char_width).round() as usize).min(text_len)
            };
            state.update_selection(char_pos);
            let selection = Self::selection_from_state(&state);
            let still_dragging = state.is_dragging;
            drop(state);
            self.emit_selection_change(selection, window, cx);
            self.reveal_cursor();
            // A pointer held near the viewport edge must keep scrolling, even
            // when no further MouseMove events arrive.
            if still_dragging {
                self.continue_edge_selection(event.position, window, cx);
            }
            window.refresh();
        }
    }

    fn handle_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut state = self.edit_state.borrow_mut();
        if state.is_dragging {
            state.end_selection();
            let selection = Self::selection_from_state(&state);
            drop(state);
            self.emit_selection_change(selection, window, cx);
            self.reveal_cursor();
            window.refresh();
        }
    }

    /// Clipboard and undo shortcuts (platform or ctrl modifier).
    fn handle_clipboard_key(
        &mut self,
        key: &str,
        shift: bool,
        ctrl: bool,
        cmd: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut state = self.edit_state.borrow_mut();
        // Platform modifier handles clipboard shortcuts and select-all. Keep
        // ctrl+a available for the Emacs start-of-line binding below.
        if (cmd && matches!(key, "c" | "x" | "v" | "a" | "z" | "y"))
            || (ctrl && matches!(key, "c" | "x" | "v" | "z" | "y"))
            || (ctrl && key == "a" && self.props.select_all_on_ctrl)
        {
            cx.stop_propagation();
            match key {
                "c" => {
                    if self.props.password {
                        return true;
                    }
                    if let Some(selected) = state.get_selected_text() {
                        drop(state);
                        cx.write_to_clipboard(ClipboardItem::new_string(selected));
                    }
                    return true;
                }
                "x" => {
                    if self.props.password {
                        return true;
                    }
                    if let Some(selected) = state.get_selected_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(selected));
                        state.delete_selection();
                        if let Some(ref handler) = self.props.on_text_change {
                            handler(&state.text, window, cx);
                        }
                        drop(state);
                        self.reveal_cursor();
                        self.reveal_cursor_next_frame(window, cx);
                        window.refresh();
                    }
                    return true;
                }
                "v" => {
                    if let Some(clipboard) = cx.read_from_clipboard()
                        && let Some(paste_text) = clipboard.text()
                    {
                        state.insert_text(&paste_text);
                        if let Some(ref handler) = self.props.on_text_change {
                            handler(&state.text, window, cx);
                        }
                        drop(state);
                        self.reveal_cursor();
                        self.reveal_cursor_next_frame(window, cx);
                        window.refresh();
                    }
                    return true;
                }
                "a" => {
                    state.select_all();
                    let selection = Self::selection_from_state(&state);
                    drop(state);
                    self.emit_selection_change(selection, window, cx);
                    self.reveal_cursor();
                    window.refresh();
                    return true;
                }
                "z" => {
                    let changed = if shift { state.redo() } else { state.undo() };
                    if changed {
                        let selection = Self::selection_from_state(&state);
                        self.emit_text_change(&state.text, window, cx);
                        drop(state);
                        self.emit_selection_change(selection, window, cx);
                        self.reveal_cursor();
                        self.reveal_cursor_next_frame(window, cx);
                        window.refresh();
                    }
                    return true;
                }
                "y" if ctrl => {
                    if state.redo() {
                        let selection = Self::selection_from_state(&state);
                        self.emit_text_change(&state.text, window, cx);
                        drop(state);
                        self.emit_selection_change(selection, window, cx);
                        self.reveal_cursor();
                        window.refresh();
                    }
                    return true;
                }
                _ => return false,
            }
        }
        false
    }

    /// Cmd+left/right line jumps (macOS); shift extends the selection.
    fn handle_line_jump_key(
        &mut self,
        key: &str,
        shift: bool,
        cmd: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut state = self.edit_state.borrow_mut();
        // cmd+left/right — line start/end (macOS); cmd+shift extends selection
        if cmd && matches!(key, "left" | "right") {
            cx.stop_propagation();
            if shift {
                match key {
                    "left" => state.extend_to_start(),
                    "right" => state.extend_to_end(),
                    _ => {}
                }
            } else {
                match key {
                    "left" => state.move_to_start(),
                    "right" => state.move_to_end(),
                    _ => {}
                }
            }
            let selection = Self::selection_from_state(&state);
            drop(state);
            self.emit_selection_change(selection, window, cx);
            self.reveal_cursor();
            window.refresh();
            return true;
        }
        false
    }

    /// Alt+left/right word jumps; shift extends the selection.
    fn handle_word_jump_key(
        &mut self,
        key: &str,
        shift: bool,
        alt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut state = self.edit_state.borrow_mut();
        // alt+left/right — word jump; alt+shift extends selection
        if alt && matches!(key, "left" | "right") {
            cx.stop_propagation();
            if shift {
                match key {
                    "left" => state.extend_word_backward(),
                    "right" => state.extend_word_forward(),
                    _ => {}
                }
            } else {
                match key {
                    "left" => state.move_word_backward(),
                    "right" => state.move_word_forward(),
                    _ => {}
                }
            }
            let selection = Self::selection_from_state(&state);
            drop(state);
            self.emit_selection_change(selection, window, cx);
            self.reveal_cursor();
            window.refresh();
            return true;
        }
        false
    }

    /// Alt+backspace / alt+d word kill.
    fn handle_kill_word_key(
        &mut self,
        key: &str,
        alt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut state = self.edit_state.borrow_mut();
        // alt+backspace / alt+d — kill word
        if alt && matches!(key, "backspace" | "d") {
            cx.stop_propagation();

            match key {
                "backspace" => {
                    state.kill_word_backward();
                    if let Some(ref handler) = self.props.on_text_change {
                        handler(&state.text, window, cx);
                    }
                    drop(state);
                    self.reveal_cursor();
                    self.reveal_cursor_next_frame(window, cx);
                    window.refresh();
                    return true;
                }
                "d" => {
                    state.kill_word_forward();
                    if let Some(ref handler) = self.props.on_text_change {
                        handler(&state.text, window, cx);
                    }
                    drop(state);
                    self.reveal_cursor();
                    self.reveal_cursor_next_frame(window, cx);
                    window.refresh();
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Emacs ctrl bindings.
    fn handle_emacs_key(
        &mut self,
        key: &str,
        shift: bool,
        ctrl: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut state = self.edit_state.borrow_mut();
        // Emacs ctrl bindings
        if ctrl
            && matches!(
                key,
                "a" | "e" | "k" | "u" | "w" | "h" | "d" | "f" | "b" | "left" | "right" | "y"
            )
        {
            cx.stop_propagation();

            match key {
                "a" => state.move_to_start(),
                "e" => state.move_to_end(),
                "k" => state.kill_to_end(),
                "u" => state.kill_to_start(),
                "w" => state.kill_word_backward(),
                "h" => state.do_backspace(),
                "d" => state.do_delete(),
                "f" => state.move_forward(),
                "b" => state.move_backward(),
                "left" => {
                    if shift {
                        state.extend_word_backward();
                    } else {
                        state.move_word_backward();
                    }
                }
                "right" => {
                    if shift {
                        state.extend_word_forward();
                    } else {
                        state.move_word_forward();
                    }
                }
                "y" => {
                    if let Some(clipboard) = cx.read_from_clipboard()
                        && let Some(paste_text) = clipboard.text()
                    {
                        state.insert_text(&paste_text);
                    }
                }
                _ => {}
            }
            if let Some(ref handler) = self.props.on_text_change {
                handler(&state.text, window, cx);
            }
            drop(state);
            self.reveal_cursor();
            self.reveal_cursor_next_frame(window, cx);
            window.refresh();
            return true;
        }
        false
    }

    /// Unmodified keys: commit, escape, editing, navigation, text entry.
    // Modifier flags mirror the key-event fields one-to-one; routing
    // them through a struct adds indirection at every call site.
    #[allow(clippy::fn_params_excessive_bools)]
    fn handle_plain_key(
        &mut self,
        key: &str,
        event: &KeyDownEvent,
        shift: bool,
        ctrl: bool,
        cmd: bool,
        alt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut state = self.edit_state.borrow_mut();
        if ctrl || cmd || alt {
            return false;
        }

        if self.props.multiline && matches!(key, "up" | "down") {
            if let Some(layout) = &self.multiline_layout {
                let visual_map = self.multiline_visual_map.borrow();
                let byte = visual_map.as_ref().map_or_else(
                    || Self::char_range_byte_offsets(&state.text, state.cursor, state.cursor).0,
                    |map| {
                        Self::char_range_byte_offsets(
                            &map.display_text,
                            map.cursor_char(state.cursor),
                            map.cursor_char(state.cursor),
                        )
                        .0
                    },
                );
                if let Some(mut point) = layout.position_for_index(byte) {
                    // Stepping exactly one line height lands on the next row's
                    // top boundary, which the hit-tester reports as
                    // out-of-line (Err(0)) and collapses the cursor to the
                    // start of the text. Probe the middle of the adjacent row
                    // instead so both directions resolve inside the target.
                    let line_height = layout.line_height();
                    point.y += if key == "up" {
                        -line_height * 0.5
                    } else {
                        line_height * 1.5
                    };
                    let target = layout.index_for_position(point).unwrap_or_else(|i| i).min(
                        visual_map
                            .as_ref()
                            .map_or(state.text.len(), |map| map.display_text.len()),
                    );
                    if shift {
                        let cursor = state.cursor;
                        state.selection_anchor.get_or_insert(cursor);
                    } else {
                        state.clear_selection();
                    }
                    state.cursor = visual_map.as_ref().map_or_else(
                        || state.text[..target].chars().count(),
                        |map| Self::original_char_for_display_byte(map, target),
                    );
                }
            }
            drop(state);
            cx.stop_propagation();
            self.reveal_cursor();
            window.refresh();
            return true;
        }

        if !matches!(
            key,
            "enter" | "escape" | "backspace" | "delete" | "left" | "right" | "home" | "end"
        ) && keystroke_to_char(&event.keystroke).is_none()
        {
            return false;
        }
        cx.stop_propagation();

        match key {
            "enter" if self.props.multiline => {
                if shift {
                    state.insert_text("\n");
                    self.emit_text_change(&state.text, window, cx);
                } else {
                    let text = state.text.clone();
                    drop(state);
                    if let Some(handler) = &self.props.on_change {
                        handler(&text, window, cx);
                    }
                }
                window.refresh();
            }
            "enter" => {
                drop(state);
                self.commit_edit(window, cx);
                window.blur(cx);
            }
            "escape" => {
                state.abandon_edit();
                drop(state);
                window.blur(cx);
                if let Some(ref handler) = self.props.on_edit_end {
                    handler(None, window, cx);
                }
            }
            "backspace" => {
                state.do_backspace();
                if let Some(ref handler) = self.props.on_text_change {
                    handler(&state.text, window, cx);
                }
                drop(state);
                self.reveal_cursor();
                self.reveal_cursor_next_frame(window, cx);
                window.refresh();
            }
            "delete" => {
                state.do_delete();
                if let Some(ref handler) = self.props.on_text_change {
                    handler(&state.text, window, cx);
                }
                drop(state);
                self.reveal_cursor();
                self.reveal_cursor_next_frame(window, cx);
                window.refresh();
            }
            "left" => {
                if shift {
                    state.extend_backward();
                } else {
                    state.move_backward();
                }
                let selection = Self::selection_from_state(&state);
                drop(state);
                self.emit_selection_change(selection, window, cx);
                self.reveal_cursor();
                window.refresh();
            }
            "right" => {
                if shift {
                    state.extend_forward();
                } else {
                    state.move_forward();
                }
                let selection = Self::selection_from_state(&state);
                drop(state);
                self.emit_selection_change(selection, window, cx);
                self.reveal_cursor();
                window.refresh();
            }
            "home" => {
                if shift {
                    state.extend_to_start();
                } else {
                    state.move_to_start();
                }
                let selection = Self::selection_from_state(&state);
                drop(state);
                self.emit_selection_change(selection, window, cx);
                self.reveal_cursor();
                window.refresh();
            }
            "end" => {
                if shift {
                    state.extend_to_end();
                } else {
                    state.move_to_end();
                }
                let selection = Self::selection_from_state(&state);
                drop(state);
                self.emit_selection_change(selection, window, cx);
                self.reveal_cursor();
                window.refresh();
            }
            _ => {
                if let Some(ch) = keystroke_to_char(&event.keystroke) {
                    state.insert_char(ch);
                    if let Some(ref handler) = self.props.on_text_change {
                        handler(&state.text, window, cx);
                    }
                    drop(state);
                    self.reveal_cursor();
                    self.reveal_cursor_next_frame(window, cx);
                    window.refresh();
                } else {
                    return false;
                }
            }
        }
        true
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) && !self.edit_state.borrow().editing {
            return;
        }
        if event.keystroke.key.as_str() == "tab" {
            return;
        }
        let key = event.keystroke.key.as_str();
        let ctrl = event.keystroke.modifiers.control;
        let cmd = event.keystroke.modifiers.platform;
        let alt = event.keystroke.modifiers.alt;
        let shift = event.keystroke.modifiers.shift;
        self.marked_text_range.borrow_mut().take();

        let mut state = self.edit_state.borrow_mut();
        if !state.editing {
            state.text = self.props.value.to_string();
            state.editing = true;
            state.cursor = state.text.chars().count();
            state.clear_selection();
        }
        // Release the borrow: each key group below re-borrows on entry so
        // `&mut self` handlers never alias a live edit borrow.
        drop(state);

        if self.handle_clipboard_key(key, shift, ctrl, cmd, window, cx) {
            return;
        }
        if self.handle_line_jump_key(key, shift, cmd, window, cx) {
            return;
        }
        if self.handle_word_jump_key(key, shift, alt, window, cx) {
            return;
        }
        if self.handle_kill_word_key(key, alt, window, cx) {
            return;
        }
        if self.handle_emacs_key(key, shift, ctrl, window, cx) {
            return;
        }
        self.handle_plain_key(key, event, shift, ctrl, cmd, alt, window, cx);
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.hovered != hovered {
            self.hovered = hovered;
            cx.notify();
        }
    }
}

impl EntityInputHandler for InputEntity {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let state = self.edit_state.borrow();
        let start = Self::utf16_to_char(&state.text, range.start);
        let end = Self::utf16_to_char(&state.text, range.end);
        let start_byte = state
            .text
            .char_indices()
            .nth(start)
            .map_or(state.text.len(), |(idx, _)| idx);
        let end_byte = state
            .text
            .char_indices()
            .nth(end)
            .map_or(state.text.len(), |(idx, _)| idx);
        *adjusted_range =
            Some(Self::char_to_utf16(&state.text, start)..Self::char_to_utf16(&state.text, end));
        Some(state.text[start_byte..end_byte].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.props.disabled || self.props.readonly {
            return None;
        }

        self.ensure_editing_state();
        let state = self.edit_state.borrow();
        let range = Self::current_selected_char_range(&state);
        Some(UTF16Selection {
            range: Self::char_to_utf16(&state.text, range.start)
                ..Self::char_to_utf16(&state.text, range.end),
            reversed: state
                .selection_anchor
                .is_some_and(|anchor| state.cursor < anchor),
        })
    }

    fn text_length_utf16(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.props.disabled || self.props.readonly {
            return None;
        }
        let state = self.edit_state.borrow();
        Some(state.text.encode_utf16().count())
    }

    /// Platform-driven selection update: the native editor reports its own
    /// selection (handle drags, Select All) so GPUI's state stays in sync.
    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.props.disabled || self.props.readonly {
            return;
        }

        self.ensure_editing_state();
        {
            let mut state = self.edit_state.borrow_mut();
            let start = Self::utf16_to_char(&state.text, range_utf16.start);
            let end = Self::utf16_to_char(&state.text, range_utf16.end).max(start);
            state.selection_anchor = Some(start);
            state.cursor = end;
        }
        let selection = {
            let state = self.edit_state.borrow();
            Self::selection_from_state(&state)
        };
        self.emit_selection_change(selection, window, cx);
        self.reveal_cursor();
        self.reveal_cursor_next_frame(window, cx);
        window.refresh();
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_text_range.borrow().clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_text_range.borrow_mut().take();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.props.disabled || self.props.readonly {
            return;
        }

        let marked_range = self.marked_text_range.borrow_mut().take();
        self.ensure_editing_state();
        let mut state = self.edit_state.borrow_mut();
        let range = Self::replacement_char_range(&state, range, marked_range);
        Self::replace_char_range(&mut state, range, text);
        let selection = Self::selection_from_state(&state);
        self.emit_text_change(&state.text, window, cx);
        drop(state);
        self.emit_selection_change(selection, window, cx);
        self.reveal_cursor();
        self.reveal_cursor_next_frame(window, cx);
        window.refresh();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.props.disabled || self.props.readonly {
            return;
        }

        self.ensure_editing_state();
        let mut state = self.edit_state.borrow_mut();
        let replacement_range =
            Self::replacement_char_range(&state, range, self.marked_text_range.borrow().clone());
        let insertion_start = replacement_range.start;
        Self::replace_char_range(&mut state, replacement_range, new_text);

        let insertion_start_utf16 = Self::char_to_utf16(&state.text, insertion_start);
        let inserted_utf16_len = new_text.encode_utf16().count();
        *self.marked_text_range.borrow_mut() = if inserted_utf16_len == 0 {
            None
        } else {
            Some(insertion_start_utf16..insertion_start_utf16 + inserted_utf16_len)
        };

        if let Some(selected_range) = new_selected_range {
            let start = insertion_start + Self::utf16_to_char(new_text, selected_range.start);
            let end = insertion_start + Self::utf16_to_char(new_text, selected_range.end);
            state.selection_anchor = Some(start);
            state.cursor = end.max(start);
        }

        let selection = Self::selection_from_state(&state);
        self.emit_text_change(&state.text, window, cx);
        drop(state);
        self.emit_selection_change(selection, window, cx);
        self.reveal_cursor();
        self.reveal_cursor_next_frame(window, cx);
        window.refresh();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let state = self.edit_state.borrow();
        let start = Self::utf16_to_char(&state.text, range_utf16.start);
        let end = Self::utf16_to_char(&state.text, range_utf16.end).max(start);
        if self.props.multiline
            && let Some(layout) = &self.multiline_layout
        {
            let visual_map = self.multiline_visual_map.borrow();
            let (start, end) = visual_map.as_ref().map_or_else(
                || Self::char_range_byte_offsets(&state.text, start, end),
                |map| {
                    let start_char = map.cursor_char(start);
                    let end_char = map.end_char(end);
                    Self::char_range_byte_offsets(&map.display_text, start_char, end_char)
                },
            );
            let origin = layout.position_for_index(start)?;
            let finish = layout.position_for_index(end).unwrap_or(origin);
            return Some(Bounds::new(
                origin,
                gpui::size((finish.x - origin.x).max(px(1.5)), px(20.0)),
            ));
        }
        let char_width = 8.0_f32;
        Some(Bounds {
            origin: gpui::point(
                element_bounds.origin.x + px(start as f32 * char_width),
                element_bounds.origin.y,
            ),
            size: gpui::size(
                px((end - start).max(1) as f32 * char_width),
                element_bounds.size.height,
            ),
        })
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let state = self.edit_state.borrow();
        if self.props.multiline
            && let Some(layout) = &self.multiline_layout
        {
            let visual_map = self.multiline_visual_map.borrow();
            let byte = layout.index_for_position(point).unwrap_or_else(|i| i).min(
                visual_map
                    .as_ref()
                    .map_or(state.text.len(), |map| map.display_text.len()),
            );
            let original_char = visual_map.as_ref().map_or_else(
                || state.text[..byte].chars().count(),
                |map| Self::original_char_for_display_byte(map, byte),
            );
            return Some(Self::char_to_utf16(&state.text, original_char));
        }
        let char_width = 8.0_f32;
        let x: f32 = point.x.into();
        Some(((x / char_width).round().max(0.0) as usize).min(state.text.chars().count()))
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        !self.props.disabled && !self.props.readonly
    }
}

struct InputElement {
    child: AnyElement,
    focus_handle: FocusHandle,
    entity: Entity<InputEntity>,
    input_id: ElementId,
}

impl Element for InputElement {
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
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        window.set_focus_handle(&self.focus_handle, cx);
        #[cfg(target_os = "android")]
        if let Some(native_focused) = native_editor_focus_for(&self.input_id) {
            let outside_tap_pending = self
                .entity
                .read_with(cx, |model, _| model.pending_outside_blur.get());
            if native_focused && !self.focus_handle.is_focused(window) {
                window.focus(&self.focus_handle, cx);
            } else if !native_focused
                && !outside_tap_pending
                && self.focus_handle.is_focused(window)
            {
                // Android reports the EditText window losing focus as soon
                // as a sibling control is touched. Keep GPUI focus through
                // that touch sequence so the sibling receives its MouseUp
                // and click; the outside MouseUp handler ends the editor
                // session after the control has had a chance to run.
                window.blur(cx);
            }
        }
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let entity = self.entity.clone();
        let focus_handle = self.focus_handle.clone();
        let paint_entity = entity.clone();
        let blur_focus = focus_handle.clone();
        let pending_outside_blur =
            entity.read_with(cx, |model, _| model.pending_outside_blur.clone());
        let pending_outside_blur_for_up = pending_outside_blur.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _cx| {
            if phase != DispatchPhase::Capture {
                return;
            }

            let should_blur = blur_focus.is_focused(window) && !bounds.contains(&event.position);
            pending_outside_blur.set(should_blur);
        });
        window.on_mouse_event({
            let blur_focus = focus_handle.clone();
            move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && pending_outside_blur_for_up.replace(false)
                    && !bounds.contains(&event.position)
                    && blur_focus.is_focused(window)
                {
                    // Let every MouseUp listener finish first. Blurring during
                    // MouseDown can remove the pressed button before its click
                    // callback runs, which made a focused Cookie field require
                    // two taps on Save/Cancel.
                    let deferred_blur_focus = blur_focus.clone();
                    window.defer(cx, move |window, cx| {
                        if deferred_blur_focus.is_focused(window) {
                            window.blur(cx);
                            #[cfg(target_os = "android")]
                            if native_editor_visible() {
                                dispatch_text_menu_blur();
                            }
                        }
                    });
                }
            }
        });
        // Cold long-press (input not focused yet): focus and hand the input
        // straight to the native editor so the next long-press produces the
        // system word selection + edit menu.
        window.on_mouse_event({
            let entity = entity.clone();
            let focus_handle = focus_handle.clone();
            move |event: &LongPressEvent, phase, window, cx| {
                if !phase.bubble() || event.phase != TouchPhase::Started {
                    return;
                }
                if !bounds.contains(&event.start_position) || focus_handle.is_focused(window) {
                    return;
                }
                let cold_press = MouseDownEvent {
                    button: MouseButton::Left,
                    position: event.start_position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                };
                let claimed = entity.update(cx, |model, cx| {
                    if model.props.disabled || model.props.readonly {
                        return false;
                    }
                    // Reuse the normal tap path so editing state, cursor
                    // placement, and the edit-start hook all run.
                    model.handle_mouse_down(&cold_press, window, cx);
                    true
                });
                if claimed {
                    // Claim the gesture so the platform does not turn the
                    // remaining movement into a scroll.
                    cx.stop_propagation();
                }
            }
        });
        // Record the painted geometry so the native editor can be positioned
        // over this exact rect, and keep it aligned while the input is
        // focused (e.g. when the soft keyboard resizes the window).
        paint_entity.update(cx, |model, _| {
            let x: f32 = bounds.origin.x.into();
            let y: f32 = bounds.origin.y.into();
            let w: f32 = bounds.size.width.into();
            let h: f32 = bounds.size.height.into();
            *model.element_bounds.borrow_mut() = Some((x, y, w, h));
            let font_size = match window.text_style().font_size {
                gpui::AbsoluteLength::Pixels(size) => f32::from(size),
                gpui::AbsoluteLength::Rems(size) => size.0 * 16.0,
            };
            model.paint_font_size.set(font_size);
        });
        let native_editor_owns_text = cfg!(any(target_os = "android", target_os = "ios"))
            && has_text_menu_hook()
            && paint_entity.read_with(cx, |model, _| model.props.native_editor_enabled);
        // Mobile's platform text view is the only visible input renderer.
        // Mount it on the first paint, before a tap, so the native field owns
        // its hint and text from the start. While unfocused it lets touches
        // fall through to GPUI, which hands focus to the mounted native field.
        let native_editor_should_update = native_editor_owns_text;
        if native_editor_owns_text && native_editor_should_update {
            paint_entity.update(cx, |model, cx| {
                // Always attempt a refresh; `show_native_editor` dedupes
                // identical payloads, and the platform-side echo guard skips
                // pushes that merely mirror what the editor already holds.
                // External value changes (Paste/Clear/restore) therefore reach
                // the editor even while it is focused.
                model.show_native_editor(false, cx);
                #[cfg(target_os = "android")]
                NATIVE_EDITOR_SCROLL_HANDLE.with(|handle| {
                    *handle.borrow_mut() = model.props.editor_scroll_handle.clone();
                });
            });
        }
        #[cfg(target_os = "android")]
        if !native_editor_owns_text
            && native_editor_visible()
            && ACTIVE_NATIVE_EDITOR_ID
                .with(|active| active.borrow().as_ref() == Some(&self.input_id))
        {
            // Android's editor is a separate application window. Hide it as
            // soon as its owning input stops rendering the native surface so
            // it cannot hover above a dialog or destination page.
            hide_text_menu();
        }
        #[cfg(target_os = "ios")]
        if !native_editor_owns_text {
            hide_text_menu_for(&self.input_id);
        }
        window.handle_input(
            &self.focus_handle,
            ElementInputHandler::new(bounds, self.entity.clone()),
            cx,
        );
        self.child.paint(window, cx);
    }
}

impl IntoElement for InputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Render for InputEntity {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let props = &self.props;

        // Build the effective label from references to avoid cloning through
        // the whole fallback chain on every render.
        let effective_label: Option<&SharedString> = props
            .aria_label
            .as_ref()
            .or(props.label.as_ref())
            .or(props.placeholder.as_ref());
        let native_label = effective_label.cloned().unwrap_or_default();
        let native_props = AriaProps::with_role(props.aria_role.unwrap_or(AriaRole::Textbox))
            .maybe_state(props.disabled, AriaState::Disabled)
            .value_text(if props.password {
                Self::cached_password_mask(&self.aria_password_mask, props.value.as_ref())
            } else {
                props.value.clone()
            });
        let global_theme = cx
            .try_global::<ThemeState>()
            .map(|state| state.theme.clone())
            .unwrap_or_else(|| std::sync::Arc::new(Theme::default()));
        let theme = InputTheme::from_theme(global_theme.as_ref());

        let (py, _text_size_class) = match props.size {
            InputSize::Xs => (px(2.0), "text_xs"),
            InputSize::Sm => (px(4.0), "text_xs"),
            InputSize::Md => (px(8.0), "text_sm"),
            InputSize::Lg => (px(12.0), "text_base"),
        };

        let effective_error = props.effective_error();
        let has_error = effective_error.is_some();
        let disabled = props.disabled;
        let readonly = props.readonly;

        // Determine editing state from focus
        let is_focused = self.focus_handle.is_focused(window);
        let editing = is_focused && !disabled && !readonly;
        self.render_focused.set(is_focused);
        let native_editor_owns_text = cfg!(any(target_os = "android", target_os = "ios"))
            && has_text_menu_hook()
            && props.native_editor_enabled
            && !disabled
            && !readonly;

        // Get display state from edit_state
        let state = self.edit_state.borrow();
        let selection_anchor = if editing {
            state.selection_anchor
        } else {
            None
        };
        let cursor_pos = state.cursor;
        let edit_text: Option<SharedString> =
            (editing && state.editing).then(|| state.text.clone().into());
        drop(state);

        let border_color = if has_error {
            theme.error
        } else if editing && props.border_color.is_none() {
            theme.border_focus
        } else {
            props.border_color.unwrap_or(theme.border)
        };

        let mut container = div().flex().flex_col().gap_1();

        // Label
        if let Some(label) = &props.label {
            container = container.child(
                div()
                    .font_family(global_theme.font_family.clone())
                    .text_sm()
                    .text_color(theme.label)
                    .font_weight(FontWeight::MEDIUM)
                    .child(label.clone()),
            );
        }

        let field_id = ElementId::from((props.id.clone(), "field"));
        let input_debug_id = props.id.to_string();

        // Input wrapper
        let mut input_wrapper = div()
            .id(props.id.clone())
            .debug_selector(move || input_debug_id)
            .font_family(global_theme.font_family.clone())
            .track_focus_element(&self.focus_handle)
            .flex()
            .items_center()
            .gap_2()
            .px(props.horizontal_padding.unwrap_or(gpui::rems(0.75)))
            .py(py)
            .rounded_md()
            .border_1()
            .border_color(border_color)
            .focusable();

        // Apply variant styling
        match props.variant {
            InputVariant::Default => {
                input_wrapper = input_wrapper.bg(props.bg_color.unwrap_or(theme.background));
            }
            InputVariant::Filled => {
                input_wrapper = input_wrapper
                    .bg(props.bg_color.unwrap_or(theme.filled_bg))
                    .border_color(theme.transparent);
            }
            InputVariant::Flushed => {
                input_wrapper = input_wrapper
                    .bg(theme.transparent)
                    .border_0()
                    .border_b_1()
                    .border_color(border_color)
                    .rounded_none();
            }
        }

        let border_hover = theme.border_hover;
        if disabled {
            input_wrapper = input_wrapper.opacity(0.5).cursor_not_allowed();
        } else if !readonly {
            input_wrapper = input_wrapper
                .cursor_text()
                .when(self.hovered && props.border_color.is_none(), |s| {
                    s.border_color(border_hover)
                });
        }
        if native_editor_owns_text {
            input_wrapper = input_wrapper.bg(gpui::rgba(0x00000000)).border_0();
        }

        let placeholder_color = props.placeholder_color.unwrap_or(theme.placeholder);
        let text_color = props.text_color.unwrap_or(theme.text);
        let selection_bg = theme.selection_bg;
        let cursor_color = theme.cursor;

        // Add event handlers
        if !disabled && !readonly {
            input_wrapper = input_wrapper
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.handle_mouse_down(event, window, cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                    this.handle_mouse_move(event, window, cx);
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseUpEvent, window, cx| {
                        this.handle_mouse_up(event, window, cx);
                    }),
                )
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.handle_key_down(event, window, cx);
                }))
                .on_hover(
                    cx.listener(|this: &mut InputEntity, hovered: &bool, _window, cx| {
                        this.set_hovered(*hovered, cx);
                    }),
                );
        }

        // Left icon
        if let Some(icon) = &props.icon_left {
            input_wrapper =
                input_wrapper.child(div().text_color(placeholder_color).child(icon.clone()));
        }

        // Determine display text. Keep it as a `SharedString` when it comes from
        // props so we don't allocate a new `String` on every render.
        let editing_text = edit_text.is_some();
        // On Android the native editor is transparent while the IME animates
        // and GPUI paints this copy instead. An empty focused field would
        // otherwise paint a blank line, so keep the placeholder on screen; it
        // is laid out by the very metrics the native editor aligns to.
        let native_preview_placeholder = cfg!(target_os = "android")
            && native_editor_owns_text
            && props.value.is_empty()
            && !props
                .placeholder
                .as_ref()
                .map_or(true, |text| text.is_empty());
        let clear_display_text: SharedString = if let Some(text) = edit_text {
            if native_preview_placeholder && text.is_empty() {
                props.placeholder.clone().unwrap_or_default()
            } else {
                text
            }
        } else if props.value.is_empty() {
            props.placeholder.clone().unwrap_or_default()
        } else {
            props.value.clone()
        };
        let display_text: SharedString =
            if props.password && (editing_text || !props.value.is_empty()) {
                Self::cached_password_mask(&self.display_password_mask, clear_display_text.as_ref())
            } else {
                clear_display_text
            };

        // Build the text element with partial selection support
        let mut text_el = div().id(field_id).flex_1().flex().items_center();

        text_el = match props.size {
            InputSize::Xs => text_el.text_xs(),
            InputSize::Sm => text_el.text_xs(),
            InputSize::Md => text_el.text_sm(),
            InputSize::Lg => text_el,
        };

        if props.multiline {
            let source_text = if display_text.is_empty() {
                " "
            } else {
                display_text.as_ref()
            };
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let len = display_text.chars().count();
            #[cfg(any(target_os = "android", target_os = "ios"))]
            let len = source_text.chars().count();
            let cursor = cursor_pos.min(len);
            let anchor = selection_anchor.unwrap_or(cursor).min(len);
            let visual_map = self.multiline_visual_map(source_text);
            let display_text: SharedString = visual_map.display_text.clone().into();
            let (start_char, end_char) = (
                visual_map.cursor_char(cursor.min(anchor)),
                visual_map.end_char(cursor.max(anchor)),
            );
            let (start, end) =
                Self::char_range_byte_offsets(&visual_map.display_text, start_char, end_char);
            let styled = gpui::StyledText::new(display_text).with_highlights(
                (editing && start != end).then_some((
                    start..end,
                    gpui::HighlightStyle {
                        color: Some(text_color.into()),
                        background_color: Some(selection_bg.into()),
                        ..Default::default()
                    },
                )),
            );
            let layout = styled.layout().clone();
            self.multiline_layout = Some(layout.clone());
            let cursor_display_char = visual_map.cursor_char(cursor);
            let cursor_byte = Self::char_range_byte_offsets(
                &visual_map.display_text,
                cursor_display_char,
                cursor_display_char,
            )
            .0;
            self.multiline_visual_map.replace(Some(visual_map));
            let text_area_entity = cx.entity();
            let min_height = px(props.min_lines as f32 * MULTILINE_LINE_HEIGHT);
            text_el = text_el
                .relative()
                .min_w_0()
                .w_full()
                .flex_col()
                .items_start()
                .line_height(px(MULTILINE_LINE_HEIGHT))
                .when(native_editor_owns_text, |text_el| {
                    text_el.h(min_height).overflow_hidden()
                })
                .when(!native_editor_owns_text, |text_el| {
                    text_el.min_h(min_height).overflow_hidden()
                })
                .text_color(
                    if props.value.is_empty() && (!editing_text || native_preview_placeholder) {
                        placeholder_color
                    } else {
                        text_color
                    },
                )
                .child(styled)
                .child(
                    gpui::canvas(
                        |_, _, _| {},
                        move |bounds, (), window, cx| {
                            let x: f32 = bounds.origin.x.into();
                            let y: f32 = bounds.origin.y.into();
                            let width: f32 = bounds.size.width.into();
                            let height: f32 = bounds.size.height.into();
                            text_area_entity.update(cx, |model, _| {
                                model.text_area_bounds.replace(Some((x, y, width, height)));
                            });
                            if editing
                                && !native_editor_owns_text
                                && start == end
                                && let Some(position) = layout.position_for_index(cursor_byte)
                            {
                                window.paint_quad(gpui::fill(
                                    gpui::Bounds::new(
                                        position,
                                        gpui::size(px(1.5), px(MULTILINE_LINE_HEIGHT)),
                                    ),
                                    cursor_color,
                                ));
                            }
                        },
                    )
                    .absolute()
                    .inset_0(),
                );
            input_wrapper = input_wrapper.items_start();
        } else if editing {
            let len = display_text.chars().count();
            let cursor_pos = cursor_pos.min(len);
            let selection_anchor = selection_anchor.map(|a| a.min(len));

            let (sel_start, sel_end) = if let Some(anchor) = selection_anchor {
                (cursor_pos.min(anchor), cursor_pos.max(anchor))
            } else {
                (cursor_pos, cursor_pos)
            };

            text_el = text_el.text_color(text_color).whitespace_nowrap();

            if sel_start == sel_end {
                let text = display_text.as_ref();
                let (cursor_byte, _) = Self::char_range_byte_offsets(text, cursor_pos, len);
                let before: SharedString = text[..cursor_byte].into();
                let after: SharedString = text[cursor_byte..].into();
                let cursor_debug_id = format!("{}-cursor", props.id);
                let cursor_height = match props.size {
                    InputSize::Xs => px(14.0),
                    InputSize::Sm => px(14.0),
                    InputSize::Md => px(20.0),
                    InputSize::Lg => px(24.0),
                };

                if !before.is_empty() {
                    text_el = text_el.child(before);
                }
                text_el = text_el.child(
                    div()
                        .debug_selector(move || cursor_debug_id)
                        .flex_none()
                        .w(px(1.5))
                        .h(cursor_height)
                        .bg(cursor_color),
                );
                if !after.is_empty() {
                    text_el = text_el.child(after);
                }
            } else {
                let text = display_text.as_ref();
                let (sel_start_byte, sel_end_byte) =
                    Self::char_range_byte_offsets(text, sel_start, sel_end);
                let before: SharedString = text[..sel_start_byte].into();
                let selected: SharedString = text[sel_start_byte..sel_end_byte].into();
                let after: SharedString = text[sel_end_byte..].into();

                if !before.is_empty() {
                    text_el = text_el.child(before);
                }
                text_el = text_el.child(div().bg(selection_bg).child(selected));
                if !after.is_empty() {
                    text_el = text_el.child(after);
                }
            }
        } else if props.value.is_empty() {
            text_el = text_el.text_color(placeholder_color).child(display_text);
        } else {
            text_el = text_el.text_color(text_color).child(display_text);
        }

        if native_editor_owns_text {
            // Android's EditText paints the text, caret, selection and hint.
            // Keep the GPUI layout tree for sizing, but make its glyph layer
            // invisible so two text renderers can never disagree or shimmer.
            text_el = text_el.opacity(0.0);
        }
        input_wrapper = input_wrapper.child(text_el);

        // Right icon
        if let Some(icon) = &props.icon_right {
            input_wrapper =
                input_wrapper.child(div().text_color(placeholder_color).child(icon.clone()));
        }

        container = container.child(apply_native_accessibility(
            input_wrapper,
            native_label,
            &native_props,
        ));

        // Error message: explicit error first, then validator output.
        if let Some(error) = effective_error {
            container = container.child(div().text_xs().text_color(theme.error).child(error));
        }

        container
    }
}

#[derive(Debug, Clone)]
struct MultilineVisualMap {
    display_text: String,
    cursor_to_display: Vec<usize>,
    end_to_display: Vec<usize>,
    display_to_original: Vec<usize>,
}

impl MultilineVisualMap {
    fn new(text: &str, ranges_utf16: &[(usize, usize)]) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let mut breaks = ranges_utf16
            .iter()
            .map(|(start, _)| InputEntity::utf16_to_char(text, *start))
            .filter(|start| {
                *start > 0 && *start < chars.len() && !matches!(chars[*start - 1], '\n' | '\r')
            })
            .collect::<Vec<_>>();
        breaks.sort_unstable();
        breaks.dedup();

        let mut display_text = String::with_capacity(text.len() + breaks.len());
        let mut cursor_to_display = vec![0; chars.len() + 1];
        let mut end_to_display = vec![0; chars.len() + 1];
        let mut display_to_original = Vec::with_capacity(chars.len() + breaks.len());
        let mut display_char = 0;
        let mut break_index = 0;

        for (original_char, ch) in chars.iter().enumerate() {
            while break_index < breaks.len() && breaks[break_index] == original_char {
                end_to_display[original_char] = display_char;
                display_text.push('\n');
                display_to_original.push(original_char);
                display_char += 1;
                cursor_to_display[original_char] = display_char;
                break_index += 1;
            }
            if break_index == 0 || breaks[break_index - 1] != original_char {
                cursor_to_display[original_char] = display_char;
                end_to_display[original_char] = display_char;
            }
            display_text.push(*ch);
            display_to_original.push(original_char);
            display_char += 1;
        }

        while break_index < breaks.len() && breaks[break_index] == chars.len() {
            end_to_display[chars.len()] = display_char;
            display_text.push('\n');
            display_to_original.push(chars.len());
            display_char += 1;
            cursor_to_display[chars.len()] = display_char;
            break_index += 1;
        }
        if break_index == 0 || breaks[break_index - 1] != chars.len() {
            cursor_to_display[chars.len()] = display_char;
            end_to_display[chars.len()] = display_char;
        }

        Self {
            display_text,
            cursor_to_display,
            end_to_display,
            display_to_original,
        }
    }

    fn cursor_char(&self, original_char: usize) -> usize {
        self.cursor_to_display
            .get(original_char)
            .copied()
            .unwrap_or_else(|| self.cursor_to_display.last().copied().unwrap_or(0))
    }

    fn end_char(&self, original_char: usize) -> usize {
        self.end_to_display
            .get(original_char)
            .copied()
            .unwrap_or_else(|| self.end_to_display.last().copied().unwrap_or(0))
    }

    fn original_char(&self, display_char: usize) -> usize {
        self.display_to_original
            .get(display_char)
            .copied()
            .unwrap_or_else(|| self.display_to_original.len())
    }
}

/// Internal entity that renders an [`Input`] with stable identity across frames.
pub struct InputEntity {
    multiline_layout: Option<gpui::TextLayout>,
    multiline_visual_map: RefCell<Option<MultilineVisualMap>>,
    props: Input,
    focus_handle: FocusHandle,
    edit_state: Rc<RefCell<EditState>>,
    hovered: bool,
    aria_password_mask: RefCell<Option<(usize, SharedString)>>,
    display_password_mask: RefCell<Option<(usize, SharedString)>>,
    /// Last painted element bounds in window logical px, used to position
    /// the native editor overlay.
    element_bounds: RefCell<Option<(f32, f32, f32, f32)>>,
    /// Painted bounds of the multiline text area (the wrap container); the
    /// native editor wraps at exactly its width.
    text_area_bounds: RefCell<Option<(f32, f32, f32, f32)>>,
    /// Paint-time text size in logical px for the native editor font.
    paint_font_size: Cell<f32>,
    /// Focus state observed in the last `Render` pass. Window focus is only
    /// available during rendering, while the native handoff is assembled at
    /// paint time.
    render_focused: Cell<bool>,
    /// Last native-editor payload sent to the platform. Scrolling only moves
    /// the shared scroll handle, so avoiding repeated JNI/layout refreshes
    /// keeps touch scrolling smooth while the keyboard is open.
    last_native_editor_request: RefCell<Option<TextMenuRequest>>,
    /// Parent-controlled value changes that did not originate from the
    /// editor must be applied by the Android EditText, even during a focus
    /// handoff race.
    native_editor_force_text_sync: Cell<bool>,
    /// Most recent value emitted by this input's own edit path; used to tell
    /// a controlled-value echo from an external Paste/Clear update.
    last_local_text_update: RefCell<Option<String>>,
    /// Android defers outside-click blur until MouseUp so the target control's
    /// click handler runs while the native EditText still owns focus.
    pending_outside_blur: Rc<Cell<bool>>,
    /// Active IME composition range in UTF-16 code units.
    marked_text_range: RefCell<Option<Range<usize>>>,
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let focus_handle = self.focus_handle.clone().unwrap_or_else(|| {
            FOCUS_HANDLES.with(|handles| {
                let mut handles = handles.borrow_mut();
                handles
                    .entry(id.clone())
                    .or_insert_with(|| cx.focus_handle())
                    .clone()
            })
        });
        let edit_state = EDIT_STATES.with(|states| {
            let mut states = states.borrow_mut();
            states
                .entry(id.clone())
                .or_insert_with(|| Rc::new(RefCell::new(EditState::default())))
                .clone()
        });

        let entity: Entity<InputEntity> = INPUT_ENTITIES.with(|map| {
            let mut map = map.borrow_mut();
            if let Some(weak) = map.get(&id)
                && let Some(entity) = weak.upgrade()
            {
                return entity;
            }
            let entity = cx.new(|_cx| InputEntity {
                multiline_layout: None,
                multiline_visual_map: RefCell::new(None),
                props: Input::new(id.clone()),
                focus_handle: focus_handle.clone(),
                edit_state: edit_state.clone(),
                hovered: false,
                aria_password_mask: RefCell::new(None),
                display_password_mask: RefCell::new(None),
                element_bounds: RefCell::new(None),
                text_area_bounds: RefCell::new(None),
                paint_font_size: Cell::new(14.0),
                render_focused: Cell::new(false),
                last_native_editor_request: RefCell::new(None),
                native_editor_force_text_sync: Cell::new(false),
                last_local_text_update: RefCell::new(None),
                pending_outside_blur: Rc::new(Cell::new(false)),
                marked_text_range: RefCell::new(None),
            });
            map.insert(id.clone(), entity.downgrade());
            entity
        });

        FOCUS_SUBS.with(|subs| {
            let mut subs = subs.borrow_mut();
            if !subs.contains_key(&id) {
                let entity_weak = entity.downgrade();
                let sub = window.on_focus_out(&focus_handle, cx, move |_event, window, cx| {
                    // Android's real EditText owns focus and opens an
                    // ActionMode window while the user long-presses to
                    // select. That transient native focus change must not
                    // tear down the editor; a deliberate tap outside is
                    // handled by the mouse-down capture below.
                    #[cfg(target_os = "android")]
                    if native_editor_visible() {
                        return;
                    }
                    if let Some(entity) = entity_weak.upgrade() {
                        entity.update(cx, |model, cx| {
                            model.commit_edit(window, cx);
                        });
                    }
                });
                subs.insert(id.clone(), sub);
            }
        });

        entity.update(cx, |model, _cx| {
            let parent_value_changed = model.props.value != self.value;
            let is_local_echo =
                model.last_local_text_update.borrow().as_deref() == Some(self.value.as_ref());
            #[cfg(target_os = "ios")]
            let is_native_echo =
                parent_value_changed && take_native_editor_text_echo(&self.id, self.value.as_ref());
            #[cfg(not(target_os = "ios"))]
            let is_native_echo = false;
            if parent_value_changed && !is_local_echo && !is_native_echo {
                model.native_editor_force_text_sync.set(true);
            } else if is_native_echo {
                model.native_editor_force_text_sync.set(false);
            }
            if parent_value_changed || is_local_echo {
                model.last_local_text_update.borrow_mut().take();
            }
            edit_state
                .borrow_mut()
                .sync_external_value(&model.props.value, &self.value);
            model.props = self;
            // Keep the persistent focus handle/edit state in sync with any
            // explicit ones provided on the builder.
            model.focus_handle = focus_handle.clone();
            model.edit_state = edit_state;
        });
        InputElement {
            child: entity.clone().into_any_element(),
            focus_handle,
            entity,
            input_id: id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EditState, InputEntity, MultilineClickTarget, multiline_click_target};

    #[test]
    fn click_inside_content_uses_layout() {
        assert_eq!(
            multiline_click_target("abc", 0, 0, 5),
            MultilineClickTarget::Layout
        );
        assert_eq!(
            multiline_click_target("abcdef", 1, 1, 5),
            MultilineClickTarget::Layout
        );
    }

    #[test]
    fn click_below_content_moves_caret_to_end() {
        assert_eq!(
            multiline_click_target("abc", 2, 0, 5),
            MultilineClickTarget::Char(3)
        );
        assert_eq!(
            multiline_click_target("abc", 20, 0, 5),
            MultilineClickTarget::Char(3)
        );
    }

    #[test]
    fn click_on_existing_trailing_blank_lines_moves_after_newlines() {
        assert_eq!(
            multiline_click_target("abc\n", 1, 0, 5),
            MultilineClickTarget::Char(4)
        );
        assert_eq!(
            multiline_click_target("abc\n\n", 1, 0, 5),
            MultilineClickTarget::Char(4)
        );
        assert_eq!(
            multiline_click_target("abc\n\n", 2, 0, 5),
            MultilineClickTarget::Char(5)
        );
    }

    #[test]
    fn click_below_existing_trailing_blank_lines_moves_to_end() {
        assert_eq!(
            multiline_click_target("abc\n\n", 4, 0, 5),
            MultilineClickTarget::Char(5)
        );
        assert_eq!(
            multiline_click_target("\n", 4, 0, 5),
            MultilineClickTarget::Char(1)
        );
    }

    #[test]
    fn ime_commit_replaces_the_marked_composition() {
        let mut state = EditState::new("ni");
        state.clear_selection();
        state.cursor = 2;

        assert_eq!(
            InputEntity::replacement_char_range(&state, None, Some(0..2)),
            0..2
        );
    }

    #[test]
    fn explicit_ime_range_takes_precedence_over_marked_range() {
        let mut state = EditState::new("prefix ni suffix");
        state.clear_selection();
        state.cursor = state.text.chars().count();

        assert_eq!(
            InputEntity::replacement_char_range(&state, Some(7..9), Some(0..2)),
            7..9
        );
    }

    #[test]
    fn ime_composition_sequence_commits_only_the_final_text() {
        let mut state = EditState::new("");
        state.clear_selection();
        state.cursor = 0;
        let mut marked_range = None;

        let range = InputEntity::replacement_char_range(&state, None, marked_range);
        InputEntity::replace_char_range(&mut state, range, "n");
        marked_range = Some(0..1);

        let range = InputEntity::replacement_char_range(&state, None, marked_range);
        InputEntity::replace_char_range(&mut state, range, "ni");
        marked_range = Some(0..2);

        let range = InputEntity::replacement_char_range(&state, None, marked_range);
        InputEntity::replace_char_range(&mut state, range, "你");

        assert_eq!(state.text, "你");
        assert_eq!(state.cursor, 1);
    }
}
