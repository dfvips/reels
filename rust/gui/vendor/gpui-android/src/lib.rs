//! Android platform backend for GPUI.
//!
//! This crate is initially ported from `itsbalamurali/gpui-mobile` and keeps
//! the Android backend separate from the existing iOS backend so we can test
//! GPUI on Android without perturbing the working iOS showcase.

pub use gpui;

pub mod event_stages;
pub mod momentum;
pub mod packages;
pub mod platform_view;

#[cfg(any(target_os = "android", test))]
mod accessibility;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "android", test))]
mod lifecycle;

#[cfg(target_os = "android")]
pub fn current_platform(headless: bool) -> std::rc::Rc<dyn gpui::Platform> {
    android::current_platform(headless)
}

#[cfg(not(target_os = "android"))]
pub fn current_platform(_headless: bool) -> std::rc::Rc<dyn gpui::Platform> {
    panic!("gpui-android can only create a platform on Android")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarContentStyle {
    Light,
    #[default]
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SystemChromeStyle {
    pub status_bar_color: Option<u32>,
    pub status_bar_style: StatusBarContentStyle,
    pub navigation_bar_color: Option<u32>,
}

impl Default for SystemChromeStyle {
    fn default() -> Self {
        Self {
            status_bar_color: None,
            status_bar_style: StatusBarContentStyle::Dark,
            navigation_bar_color: None,
        }
    }
}

pub fn set_system_chrome(style: &SystemChromeStyle) {
    #[cfg(target_os = "android")]
    {
        android::jni::set_system_chrome(style);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = style;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardType {
    #[default]
    Default,
    EmailAddress,
    Phone,
    NumberPad,
    URL,
    Decimal,
}

pub fn show_keyboard() {
    show_keyboard_with_type(KeyboardType::Default);
}

pub fn show_keyboard_with_type(keyboard_type: KeyboardType) {
    #[cfg(target_os = "android")]
    {
        android::jni::show_keyboard_android(keyboard_type);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = keyboard_type;
    }
}

pub fn hide_keyboard() {
    #[cfg(target_os = "android")]
    {
        android::jni::hide_keyboard_android();
    }
}

/// Height of the Android input-method window in logical pixels.
///
/// This is the real `WindowInsets.Type.ime()` bottom inset. It is zero while
/// the keyboard is hidden and follows the keyboard when its visible height
/// changes.
#[cfg(target_os = "android")]
pub fn ime_bottom_inset_logical() -> f32 {
    android::jni::platform()
        .and_then(|platform| platform.primary_window())
        .map(|window| window.ime_insets_logical().bottom)
        .unwrap_or(0.0)
}

/// Stable system-bar / display-cutout insets in logical pixels
/// `(left, top, right, bottom)`.
///
/// Unlike the keyboard-aware visible bounds, these values only change when
/// the system bars or the cutout actually move, so they are safe to use for
/// layout during page transitions and IME animations.
#[cfg(target_os = "android")]
pub fn safe_area_insets_logical() -> (f32, f32, f32, f32) {
    android::jni::platform()
        .and_then(|platform| platform.primary_window())
        .map(|window| {
            let insets = window.safe_area_insets_logical();
            (insets.left, insets.top, insets.right, insets.bottom)
        })
        .unwrap_or((0.0, 0.0, 0.0, 0.0))
}

/// Current display rotation as `Surface.ROTATION_*` (0=portrait, 1=landscape
/// counter-clockwise, 2=portrait upside down, 3=landscape clockwise).
#[cfg(target_os = "android")]
pub fn display_rotation() -> i32 {
    android::jni::display_rotation_android().unwrap_or(0)
}

/// End the native editor's focus/IME session (editor stays mounted and
/// visible; taps over its bounds fall through to GPUI again).
pub fn blur_native_editor() {
    #[cfg(target_os = "android")]
    {
        android::jni::blur_native_editor_android();
    }
}

/// Clear the persistent native editor along with a controlled Clear action.
pub fn clear_native_editor_text() {
    #[cfg(target_os = "android")]
    {
        android::jni::clear_native_editor_text_android();
    }
}

/// Discard a closed dialog's native draft before its input ID is reused.
///
/// This runs on the Android UI thread in the same queue as editor updates,
/// so a pending text-mirror callback from the dismissed field cannot restore
/// the unsaved value on the next frame.
#[cfg(target_os = "android")]
pub fn reset_native_editor_text(text: &str) {
    crate::discard_pending_mirror_events();
    let session = NATIVE_EDITOR_SESSION.fetch_add(1, Ordering::Relaxed) + 1;
    let text = text.to_owned();
    android::jni::reset_native_editor_text_android(&text, session);
    // A UI-thread callback can be queued in the same frame as dismissal.
    // Drain mirror events again after its JNI post has been enqueued.
    crate::discard_pending_mirror_events();
}

#[cfg(target_os = "android")]
pub(crate) fn native_editor_session() -> u64 {
    NATIVE_EDITOR_SESSION.load(Ordering::Relaxed)
}

/// Adopt a newer Java session when a UI-thread callback overtakes Rust reset.
#[cfg(target_os = "android")]
pub(crate) fn set_native_editor_session(session: u64) {
    NATIVE_EDITOR_SESSION.fetch_max(session, Ordering::AcqRel);
}

/// Synchronize Java's mirror callbacks with the current Rust session.
#[cfg(target_os = "android")]
pub fn synchronize_native_editor_session() {
    let session = native_editor_session();
    android::jni::set_native_editor_session_android(session);
}

/// Reset a dismissed mobile dialog's native editor on every platform.
#[cfg(not(target_os = "android"))]
pub fn reset_native_editor_text(_text: &str) {}

/// Drop any Android IME mirror events queued by a dismissed dialog.
pub fn discard_native_mirror_events() {
    #[cfg(target_os = "android")]
    crate::discard_pending_mirror_events();
}

/// Parameters for showing the real native editor over a focused GPUI input.
///
/// The host activity overlays a genuine `EditText` on the input's bounds so
/// the system owns the keyboard, caret, selection handles, and edit menu.
#[derive(Debug, Clone, Default)]
pub struct NativeEditorParams {
    pub text: String,
    /// Selection in UTF-16 code units.
    pub selection_utf16: (usize, usize),
    /// Element bounds in logical window px: (x, y, width, height).
    pub bounds: (f32, f32, f32, f32),
    /// Optional window-space clip rectangle used while another GPUI page is
    /// sliding above the native editor window.
    pub clip_bounds: Option<(f32, f32, f32, f32)>,
    /// Text start position in logical window px.
    pub text_origin: (f32, f32),
    pub font_size: f32,
    pub multiline: bool,
    /// GPUI-measured wrap width of the text area in logical px (`0.0` when
    /// unknown). The native editor uses the same content width so line
    /// breaks and selection handles stay aligned on every device.
    pub wrap_width: f32,
    /// GPUI multiline line height in logical px (`0.0` for single-line
    /// inputs); the native editor matches it so row spacing stays equal.
    pub line_height: f32,
    /// GPUI visual line ranges in UTF-16 offsets.
    pub line_ranges_utf16: Vec<(usize, usize)>,
    /// `true` to take focus and raise the keyboard; `false` refreshes
    /// position/text/selection only.
    pub initial: bool,
    /// Apply the incoming text even if the native editor still has focus.
    /// Used for controlled-value updates such as external Paste/Clear.
    pub force_text_sync: bool,
    /// Colors as `0xRRGGBBAA`.
    pub text_color: u32,
    pub background_color: u32,
    pub border_color: u32,
    pub focus_border_color: u32,
    pub border_width: f32,
    pub placeholder: Option<String>,
    pub placeholder_color: u32,
    pub corner_radius: f32,
}

/// Show (or refresh) the native editor takeover for the focused input.
pub fn show_native_editor(params: NativeEditorParams) {
    #[cfg(target_os = "android")]
    {
        NATIVE_EDITOR_ACTIVE.store(true, Ordering::Release);
        android::jni::record_native_editor_bounds(Some(params.bounds));
        android::jni::show_native_editor_android(params);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = params;
    }
}

/// Hide the native editor takeover and restore normal GPUI input handling.
pub fn hide_native_editor() {
    #[cfg(target_os = "android")]
    {
        NATIVE_EDITOR_ACTIVE.store(false, Ordering::Release);
        android::jni::record_native_editor_bounds(None);
        android::jni::hide_native_editor_android();
    }
}

/// Remove the native editor window synchronously while its owning dialog is
/// closing. This prevents the separate Android window from outliving the
/// dialog frame by one vsync.
pub fn hide_native_editor_now() {
    #[cfg(target_os = "android")]
    {
        NATIVE_EDITOR_ACTIVE.store(false, Ordering::Release);
        android::jni::record_native_editor_bounds(None);
        android::jni::hide_native_editor_now();
    }
}

/// Translate the native editor window with a moving dialog in the same frame.
pub fn translate_native_editor(delta_y: f32) {
    #[cfg(target_os = "android")]
    android::jni::translate_native_editor_android(delta_y);
    #[cfg(not(target_os = "android"))]
    let _ = delta_y;
}

/// Ask the Android editor window to follow IME inset changes by itself.
pub fn set_native_editor_ime_follow(follow: bool) {
    #[cfg(target_os = "android")]
    android::jni::set_native_editor_ime_follow_android(follow);
    #[cfg(not(target_os = "android"))]
    let _ = follow;
}

/// Enable the Android-only IME animation handoff used by the Cookie dialog.
///
/// While enabled, the native editor keeps its text, caret and IME session but
/// becomes invisible during the keyboard animation. GPUI draws the matching
/// visual field in the dialog surface, so it moves with the card on every
/// frame. The native editor is revealed after the final GPUI frame submits.
pub fn set_native_editor_ime_preview(preview: bool) {
    #[cfg(target_os = "android")]
    android::jni::set_native_editor_ime_preview_android(preview);
    #[cfg(not(target_os = "android"))]
    let _ = preview;
}

/// Whether Rust should currently paint the Android-only Cookie input preview.
pub fn native_editor_ime_preview_visible() -> bool {
    #[cfg(target_os = "android")]
    {
        android::jni::native_editor_ime_preview_visible()
    }
    #[cfg(not(target_os = "android"))]
    {
        false
    }
}

/// Keep the Android editor window attached but invisible while a dialog
/// prepares its separate compositor surface. Clearing the flag reveals it
/// without another addView/layout pass.
pub fn set_native_editor_warmup(warmup: bool) {
    #[cfg(target_os = "android")]
    android::jni::set_native_editor_warmup_android(warmup);
    #[cfg(not(target_os = "android"))]
    let _ = warmup;
}

/// Content and styling for the native Android Cookie dialog panel.
///
/// The panel draws the whole card — title, hint, editor and actions — inside
/// one application window. Card and editor therefore share a single
/// compositor surface and move together with the keyboard by construction.
/// Sizes are logical pixels; colors are `0xRRGGBBAA`.
#[derive(Debug, Clone, Default)]
pub struct NativeCookiePanelSpec {
    pub text: String,
    pub width: f32,
    pub title: String,
    pub hint: String,
    pub placeholder: String,
    pub cancel_label: String,
    pub save_label: String,
    pub card_bg: u32,
    pub text_color: u32,
    pub muted_color: u32,
    pub field_bg: u32,
    pub field_border: u32,
    pub field_focus: u32,
    pub primary: u32,
    pub primary_text: u32,
    pub cancel_bg: u32,
    pub cancel_border: u32,
    pub cancel_text: u32,
}

/// Show (or refresh) the native Android Cookie dialog panel.
pub fn show_native_cookie_panel(spec: &NativeCookiePanelSpec) {
    #[cfg(target_os = "android")]
    android::jni::show_native_cookie_panel_android(spec);
    #[cfg(not(target_os = "android"))]
    let _ = spec;
}

/// Hide the native Android Cookie dialog panel.
pub fn hide_native_cookie_panel() {
    #[cfg(target_os = "android")]
    android::jni::hide_native_cookie_panel_android();
}

/// Read the live contents of the platform native editor, if one is visible.
///
/// This is used during focus changes so the final keystrokes are committed
/// even when the mirror callback has not reached the GPUI frame yet.
pub fn native_editor_text() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        android::jni::native_editor_text_android()
    }
    #[cfg(not(target_os = "android"))]
    {
        None
    }
}

/// Whether the host currently owns keyboard input through a native editor.
#[cfg(target_os = "android")]
pub(crate) fn native_editor_active() -> bool {
    NATIVE_EDITOR_ACTIVE.load(Ordering::Acquire)
}

#[cfg(target_os = "android")]
static NATIVE_EDITOR_OWNS_WINDOW_FOCUS: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
pub(crate) fn native_editor_owns_window_focus() -> bool {
    NATIVE_EDITOR_OWNS_WINDOW_FOCUS.load(Ordering::Acquire)
}

#[cfg(target_os = "android")]
pub(crate) fn set_native_editor_owns_window_focus(owns_focus: bool) {
    NATIVE_EDITOR_OWNS_WINDOW_FOCUS.store(owns_focus, Ordering::Release);
}

/// Whether an active touch is extending a text selection (claimed long
/// press). While set, the touch gesture machine must not turn movements
/// into scrolls so the input can keep receiving them.
#[allow(dead_code)]
pub static TEXT_SELECT_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
static NATIVE_EDITOR_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Mirror callbacks carry the UI-thread editor session they came from. A
/// dismissed dialog increments this number so its queued text cannot land in
/// a newly opened dialog.
#[cfg(target_os = "android")]
static NATIVE_EDITOR_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "android")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEditorLineRanges {
    pub text: String,
    pub ranges_utf16: Vec<(usize, usize)>,
}

#[cfg(target_os = "android")]
static NATIVE_EDITOR_LINE_RANGES: LazyLock<Mutex<Option<NativeEditorLineRanges>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(target_os = "android")]
pub fn native_editor_line_ranges() -> Option<NativeEditorLineRanges> {
    NATIVE_EDITOR_LINE_RANGES.lock().clone()
}

#[cfg(target_os = "android")]
pub(crate) fn set_native_editor_line_ranges(ranges: Option<NativeEditorLineRanges>) {
    *NATIVE_EDITOR_LINE_RANGES.lock() = ranges;
}

/// Mark the current touch as a text-selection drag (see [`TEXT_SELECT_ACTIVE`]).
pub fn set_text_selecting(active: bool) {
    #[cfg(target_os = "android")]
    TEXT_SELECT_ACTIVE.store(active, Ordering::Release);
    #[cfg(not(target_os = "android"))]
    let _ = active;
}

#[cfg(target_os = "android")]
use parking_lot::Mutex;
use std::cell::RefCell;
#[cfg(target_os = "android")]
use std::collections::VecDeque;
#[cfg(target_os = "android")]
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

type TextInputCallbackFn = Box<dyn FnMut(&str)>;

pub static TEXT_INPUT_DIRTY: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
#[derive(Debug, Clone)]
pub(crate) enum ImeEvent {
    Commit(String),
    SetComposing(String),
    FinishComposing,
    DeleteSurrounding {
        before: usize,
        after: usize,
    },
    /// The system text menu's copy action was tapped: copy the focused
    /// input's selection into the Android clipboard on the next frame.
    CopySelection,
    /// The system text menu's cut action was tapped: copy the focused
    /// input's selection into the clipboard and delete it.
    CutSelection,
    /// The native mirror editor's text changed (system handles/menu edited
    /// the mirrored text): sync the full text and selection into the
    /// focused GPUI input.
    MirrorText {
        text: String,
        sel_start_utf16: usize,
        sel_end_utf16: usize,
    },
    /// The native mirror editor's selection changed (system handles were
    /// dragged): sync the selection into the focused GPUI input.
    MirrorSelection {
        start_utf16: usize,
        end_utf16: usize,
    },
    /// The native mirror editor scrolled its viewport. GPUI renders the
    /// visible input content, so the platform offset must drive its scroll
    /// handle as well as the native caret/selection logic.
    MirrorScroll {
        scroll_top: f32,
    },
    /// Android dismissed the native editor while hiding the IME (for
    /// example with the system Back gesture). Rust must forget the takeover
    /// so the next tap can hand the focused input to a fresh native editor.
    NativeEditorHidden,
    /// The real EditText gained or lost Android focus.
    NativeEditorFocusChanged(bool),
}

#[cfg(target_os = "android")]
static IME_EVENTS: LazyLock<Mutex<VecDeque<ImeEvent>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

#[cfg(target_os = "android")]
pub(crate) fn enqueue_ime_event(event: ImeEvent) {
    let mut events = IME_EVENTS.lock();
    // Native-editor text and selection callbacks can arrive many times
    // between two frames (especially while dragging selection handles).
    // Only the latest full snapshot matters, so collapse older pending
    // snapshots instead of replaying a long stale queue on the next frame.
    if matches!(event, ImeEvent::MirrorText { .. }) {
        // Every text snapshot includes its selection. A newer snapshot also
        // supersedes selection notifications that were queued for older text.
        events.retain(|queued| {
            !matches!(
                queued,
                ImeEvent::MirrorText { .. } | ImeEvent::MirrorSelection { .. }
            )
        });
    } else if matches!(event, ImeEvent::MirrorSelection { .. }) {
        events.retain(|queued| !matches!(queued, ImeEvent::MirrorSelection { .. }));
    } else if matches!(event, ImeEvent::MirrorScroll { .. }) {
        events.retain(|queued| !matches!(queued, ImeEvent::MirrorScroll { .. }));
    }
    events.push_back(event);
    drop(events);
    TEXT_INPUT_DIRTY.store(true, Ordering::Release);
}

#[cfg(target_os = "android")]
pub(crate) fn drain_ime_events() -> Vec<ImeEvent> {
    let mut events = IME_EVENTS.lock();
    if events.is_empty() {
        return Vec::new();
    }
    events.drain(..).collect()
}

#[cfg(target_os = "android")]
pub(crate) fn prepend_ime_events(events: Vec<ImeEvent>) {
    if events.is_empty() {
        return;
    }
    let mut queue = IME_EVENTS.lock();
    for event in events.into_iter().rev() {
        queue.push_front(event);
    }
    TEXT_INPUT_DIRTY.store(true, Ordering::Release);
}

/// Drop mirror-editor snapshots that were queued before a synchronous
/// commit read the live native text.
///
/// `MirrorText` callbacks are frame-batched. If one is drained after the
/// input has already committed and hidden its native editor, replaying the
/// stale snapshot can resurrect an editing session with an empty editor on
/// Android. The live `EditText` value is authoritative at commit time, so
/// anything queued before that read is obsolete.
#[cfg(target_os = "android")]
pub(crate) fn discard_pending_mirror_events() {
    let mut events = IME_EVENTS.lock();
    events.retain(|event| {
        !matches!(
            event,
            ImeEvent::MirrorText { .. }
                | ImeEvent::MirrorSelection { .. }
                | ImeEvent::MirrorScroll { .. }
        )
    });
}

type NativeEditorScrollCallbackFn = Box<dyn Fn(f32)>;
type NativeEditorHiddenCallbackFn = Box<dyn Fn()>;
type NativeEditorFocusCallbackFn = Box<dyn Fn(bool)>;

thread_local! {
    static NATIVE_EDITOR_SCROLL_CALLBACK: RefCell<Option<NativeEditorScrollCallbackFn>> =
        RefCell::new(None);
    static NATIVE_EDITOR_HIDDEN_CALLBACK: RefCell<Option<NativeEditorHiddenCallbackFn>> =
        RefCell::new(None);
    static NATIVE_EDITOR_FOCUS_CALLBACK: RefCell<Option<NativeEditorFocusCallbackFn>> =
        RefCell::new(None);
}

/// Register the GPUI-side receiver for native editor scroll offsets.
#[cfg(target_os = "android")]
pub fn set_native_editor_scroll_callback(callback: Option<NativeEditorScrollCallbackFn>) {
    NATIVE_EDITOR_SCROLL_CALLBACK.with(|slot| {
        *slot.borrow_mut() = callback;
    });
}

#[cfg(target_os = "android")]
pub(crate) fn dispatch_native_editor_scroll(scroll_top: f32) {
    NATIVE_EDITOR_SCROLL_CALLBACK.with(|slot| {
        if let Some(callback) = slot.borrow().as_ref() {
            callback(scroll_top);
        }
    });
}

/// Register the GPUI-side receiver for native editor dismissal.
#[cfg(target_os = "android")]
pub fn set_native_editor_hidden_callback(callback: Option<NativeEditorHiddenCallbackFn>) {
    NATIVE_EDITOR_HIDDEN_CALLBACK.with(|slot| {
        *slot.borrow_mut() = callback;
    });
}

#[cfg(target_os = "android")]
pub(crate) fn dispatch_native_editor_hidden() {
    NATIVE_EDITOR_HIDDEN_CALLBACK.with(|slot| {
        if let Some(callback) = slot.borrow().as_ref() {
            callback();
        }
    });
}

/// Register the GPUI-side receiver for native EditText focus changes.
#[cfg(target_os = "android")]
pub fn set_native_editor_focus_callback(callback: Option<NativeEditorFocusCallbackFn>) {
    NATIVE_EDITOR_FOCUS_CALLBACK.with(|slot| {
        *slot.borrow_mut() = callback;
    });
}

#[cfg(target_os = "android")]
pub(crate) fn dispatch_native_editor_focus(focused: bool) {
    NATIVE_EDITOR_FOCUS_CALLBACK.with(|slot| {
        if let Some(callback) = slot.borrow().as_ref() {
            callback(focused);
        }
    });
}

thread_local! {
    static TEXT_INPUT_CALLBACK: RefCell<Option<TextInputCallbackFn>> = RefCell::new(None);
}

pub fn set_text_input_callback(callback: Option<TextInputCallbackFn>) {
    TEXT_INPUT_CALLBACK.with(|cb| {
        *cb.borrow_mut() = callback;
    });
}

pub fn dispatch_text_input(text: &str) -> bool {
    TEXT_INPUT_CALLBACK.with(|cb| {
        if let Some(callback) = cb.borrow_mut().as_mut() {
            callback(text);
            TEXT_INPUT_DIRTY.store(true, Ordering::Release);
            true
        } else {
            false
        }
    })
}

type BackHandlerFn = Box<dyn FnMut()>;

thread_local! {
    static BACK_HANDLER: RefCell<Option<BackHandlerFn>> = RefCell::new(None);
}

/// A dialog action requested by the native Android Cookie panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeCookieAction {
    Save(String),
    Cancel,
}

type NativeCookieHandlerFn = Box<dyn FnMut(NativeCookieAction)>;

thread_local! {
    static NATIVE_COOKIE_HANDLER: RefCell<Option<NativeCookieHandlerFn>> = RefCell::new(None);
}

/// Register the handler invoked when the native Cookie panel's actions are
/// tapped. The callback runs on the Android main thread.
#[cfg(target_os = "android")]
pub fn set_native_cookie_handler(handler: Option<NativeCookieHandlerFn>) {
    NATIVE_COOKIE_HANDLER.with(|slot| {
        *slot.borrow_mut() = handler;
    });
}

/// Deliver a native Cookie panel action to the registered handler.
#[cfg(target_os = "android")]
pub fn dispatch_native_cookie_action(action: NativeCookieAction) -> bool {
    NATIVE_COOKIE_HANDLER.with(|handler| {
        if let Some(callback) = handler.borrow_mut().as_mut() {
            callback(action);
            true
        } else {
            false
        }
    })
}

/// Register the application-level handler for Android's back key / gesture.
///
/// The callback is invoked on the Android main thread. Callers should queue
/// GPUI work instead of mutating application state directly.
#[cfg(target_os = "android")]
pub fn set_back_handler(callback: Option<BackHandlerFn>) {
    BACK_HANDLER.with(|handler| {
        *handler.borrow_mut() = callback;
    });
}

/// Finish the current Android task and terminate the app process.
#[cfg(target_os = "android")]
pub fn exit_app() {
    android::jni::exit_app_android();
}

/// Finish the current Android task and terminate the app process.
#[cfg(not(target_os = "android"))]
pub fn exit_app() {}

/// Deliver a back request to the registered application handler.
///
/// Returns `true` when a handler consumed the request.
#[cfg(target_os = "android")]
pub fn dispatch_back_handler() -> bool {
    BACK_HANDLER.with(|handler| {
        if let Some(callback) = handler.borrow_mut().as_mut() {
            callback();
            true
        } else {
            false
        }
    })
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn credential_alias(service: &str, username: &str) -> String {
    // Stable FNV-1a: aliases must survive process restarts and app upgrades.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in service
        .as_bytes()
        .iter()
        .chain([0xff].iter())
        .chain(username.as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("gpui_{hash:016x}")
}

#[cfg(test)]
mod credential_alias_tests {
    use super::credential_alias;

    #[test]
    fn credential_alias_is_stable_and_namespaces_service_and_username() {
        assert_eq!(
            credential_alias("https://example.com", "alice"),
            credential_alias("https://example.com", "alice")
        );
        assert_ne!(
            credential_alias("https://example.com", "alice"),
            credential_alias("https://example.com", "bob")
        );
        assert_ne!(
            credential_alias("https://example.com", "alice"),
            credential_alias("https://other.example", "alice")
        );
    }
}
