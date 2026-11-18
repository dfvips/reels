//! Swift-owned native text fields for the Home and XHS Cookie inputs.
//!
//! GPUI continues to own layout and business state. A native `UITextView`
//! owns rendering, first responder state, the system keyboard, selection,
//! edit menus, paste/cut/clear mutations, and scrolling. Mutations are queued
//! here and applied to `AppView` on the next render.

use douyin_input::TextMenuRequest;
use gpui::Rgba;
use std::collections::{HashMap, VecDeque};
use std::ffi::{c_char, CStr, CString};
use std::sync::{atomic::Ordering, Mutex, OnceLock};

#[cfg(target_os = "ios")]
pub const TEXT_PADDING: f32 = 12.0;

#[derive(Clone)]
pub struct TextEvent {
    pub id: String,
    pub text: String,
}

#[derive(Clone)]
pub struct FocusEvent {
    pub id: String,
    pub focused: bool,
}

#[derive(Clone)]
pub struct HeightEvent {
    pub id: String,
    pub text: String,
    pub height: f32,
}

pub struct NativeInputEvents {
    pub text: Vec<TextEvent>,
    pub focus: Vec<FocusEvent>,
    pub height: Vec<HeightEvent>,
}

#[derive(Default)]
struct NativeInputState {
    events: VecDeque<TextEvent>,
    focus_events: VecDeque<FocusEvent>,
    height_events: VecDeque<HeightEvent>,
    text_by_id: HashMap<String, String>,
    pending_text_sync: HashMap<String, String>,
    selection_by_id: HashMap<String, (usize, usize)>,
    active_id: Option<String>,
    editing: bool,
}

static STATE: OnceLock<Mutex<NativeInputState>> = OnceLock::new();

fn state() -> &'static Mutex<NativeInputState> {
    STATE.get_or_init(|| Mutex::new(NativeInputState::default()))
}

fn c_string(value: &str) -> CString {
    CString::new(value.replace('\0', "")).expect("NUL bytes were removed")
}

unsafe fn read_c_string(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned(),
    )
}

unsafe extern "C" {
    fn reels_ios_native_input_show(
        id: *const c_char,
        text: *const c_char,
        selection_start: usize,
        selection_length: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        clip_x: f32,
        clip_y: f32,
        clip_width: f32,
        clip_height: f32,
        text_x: f32,
        text_y: f32,
        font_size: f32,
        multiline: i32,
        wrap_width: f32,
        line_height: f32,
        focus: i32,
        focused: i32,
        force_text_sync: i32,
        text_color: u32,
        background_color: u32,
        border_color: u32,
        focus_border_color: u32,
        border_width: f32,
        placeholder: *const c_char,
        placeholder_color: u32,
        tint_color: u32,
        corner_radius: f32,
        min_height: f32,
        max_height: f32,
    );
    fn reels_ios_native_input_hide(id: *const c_char);
    fn reels_ios_native_input_move(id: *const c_char, x: f32, y: f32);
    fn reels_ios_native_input_hide_except(id: *const c_char);
    fn reels_ios_native_input_end_editing(id: *const c_char);
    fn reels_ios_native_input_focus(id: *const c_char);
    fn reels_ios_native_input_cancel_composing(id: *const c_char);
    fn reels_ios_native_input_replace_text(id: *const c_char, text: *const c_char);
    fn reels_ios_native_input_paste(id: *const c_char);
}

pub fn show(request: &TextMenuRequest) {
    let id = c_string(&request.id);
    let text = c_string(&request.text);
    let placeholder = c_string(request.placeholder.as_deref().unwrap_or(""));
    let (clip_x, clip_y, clip_width, clip_height) = request
        .clip_bounds
        .map_or((-1.0, -1.0, -1.0, -1.0), |bounds| bounds);

    {
        let mut state = state().lock().expect("native input state poisoned");
        if request.initial {
            state.active_id = Some(request.id.clone());
            state.editing = true;
        }
        if request.initial || request.force_text_sync || !state.text_by_id.contains_key(&request.id)
        {
            state
                .text_by_id
                .insert(request.id.clone(), request.text.clone());
        }
        state
            .selection_by_id
            .insert(request.id.clone(), request.selection_utf16);
    }

    unsafe {
        reels_ios_native_input_show(
            id.as_ptr(),
            text.as_ptr(),
            request.selection_utf16.0,
            request
                .selection_utf16
                .1
                .saturating_sub(request.selection_utf16.0),
            request.bounds.0,
            request.bounds.1,
            request.bounds.2,
            request.bounds.3,
            clip_x,
            clip_y,
            clip_width,
            clip_height,
            request.text_origin.0,
            request.text_origin.1,
            request.font_size,
            i32::from(request.multiline),
            request.wrap_width,
            request.line_height,
            i32::from(request.initial),
            i32::from(request.focused),
            i32::from(request.force_text_sync),
            request.text_color,
            request.background_color,
            request.border_color,
            request.focus_border_color,
            request.border_width,
            placeholder.as_ptr(),
            request.placeholder_color,
            request.focus_border_color,
            request.corner_radius,
            request.min_height,
            request.max_height,
        );
    }
}

/// Show a pure UIKit input over a GPUI layout slot. The slot contains no
/// Input widget, border, text or scrollbar; it only supplies frame geometry.
#[cfg(target_os = "ios")]
pub fn show_placeholder(
    id: &str,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    max_height: f32,
    min_height: f32,
    clip_bounds: Option<(f32, f32, f32, f32)>,
    palette: crate::theme::Palette,
    focused: bool,
    error: bool,
) {
    let current_id = c_string(id);
    unsafe {
        reels_ios_native_input_hide_except(current_id.as_ptr());
    }
    let force_text_sync = {
        let mut state = state().lock().expect("native input state poisoned");
        let force = state.text_by_id.get(id) != Some(&text.to_string());
        state.text_by_id.insert(id.to_string(), text.to_string());
        state
            .pending_text_sync
            .insert(id.to_string(), text.to_string());
        force
    };
    let request = TextMenuRequest {
        id: id.to_string(),
        text: text.to_string(),
        force_text_sync,
        selection_utf16: (text.encode_utf16().count(), text.encode_utf16().count()),
        bounds: (x, y, width, height),
        clip_bounds,
        text_origin: (x + TEXT_PADDING, y + TEXT_PADDING),
        font_size: 16.0,
        multiline: true,
        wrap_width: (width - TEXT_PADDING * 2.0).max(1.0),
        line_height: douyin_input::MULTILINE_LINE_HEIGHT,
        line_ranges_utf16: Vec::new(),
        initial: false,
        text_color: rgba_u32(palette.text),
        background_color: 0x00000000,
        border_color: if error {
            crate::dialogs::COOKIE_ERROR_RGBA
        } else {
            rgba_u32(if focused {
                palette.input_focus
            } else {
                palette.input_border
            })
        },
        focus_border_color: if error {
            crate::dialogs::COOKIE_ERROR_RGBA
        } else {
            rgba_u32(palette.input_focus)
        },
        border_width: 1.0,
        placeholder: Some(if id == dialogs_placeholder_id() {
            crate::dialogs::XHS_COOKIE_PLACEHOLDER.to_string()
        } else {
            "输入或粘贴短视频分享链接，支持多链接解析".to_string()
        }),
        placeholder_color: rgba_u32(palette.hint),
        corner_radius: 6.0,
        focused,
        max_height: max_height.max(1.0),
        min_height: min_height.clamp(1.0, max_height.max(1.0)),
    };
    show(&request);
}

#[cfg(target_os = "ios")]
fn dialogs_placeholder_id() -> &'static str {
    crate::dialogs::XHS_COOKIE_INPUT_ID
}

#[cfg(target_os = "ios")]
fn rgba_u32(color: Rgba) -> u32 {
    u32::from(color)
}

#[cfg(target_os = "ios")]
pub fn hide_placeholder(id: &str) {
    let mut state = state().lock().expect("native input state poisoned");
    if state.active_id.as_deref() == Some(id) {
        state.active_id = None;
        state.editing = false;
    }
    state.events.retain(|event| event.id.as_str() != id);
    state.focus_events.retain(|event| event.id.as_str() != id);
    state.height_events.retain(|event| event.id.as_str() != id);
    drop(state);
    let id = c_string(id);
    unsafe {
        reels_ios_native_input_hide(id.as_ptr());
    }
}

/// Cancel any UIKit composing/candidate state before a Rust-side value
/// mutation. Clearing while the IME is active otherwise commits the candidate
/// during keyboard dismissal and resurrects the deleted characters.
#[cfg(target_os = "ios")]
pub fn finish_editing(id: &str) {
    let mut state = state().lock().expect("native input state poisoned");
    state.events.retain(|event| event.id.as_str() != id);
    state.focus_events.retain(|event| event.id.as_str() != id);
    state.height_events.retain(|event| event.id.as_str() != id);
    drop(state);
    let id = c_string(id);
    unsafe {
        reels_ios_native_input_end_editing(id.as_ptr());
    }
}

/// Drop pending UIKit text callbacks and suppress the IME's resignation
/// commit for one frame. Paste/Clear owns the next authoritative value.
#[cfg(target_os = "ios")]
pub fn cancel_composing(id: &str) {
    let mut state = state().lock().expect("native input state poisoned");
    state.events.retain(|event| event.id.as_str() != id);
    state.height_events.retain(|event| event.id.as_str() != id);
    drop(state);
    let id = c_string(id);
    unsafe {
        reels_ios_native_input_cancel_composing(id.as_ptr());
    }
}

/// Apply an external value to a mounted UITextView immediately. A click that
/// simultaneously dismisses the IME can otherwise commit its candidate after
/// the next GPUI frame.
#[cfg(target_os = "ios")]
pub fn replace_native_text(id: &str, text: &str) {
    cancel_composing(id);
    set_text(id, text);
    let id = c_string(id);
    let text = c_string(text);
    unsafe {
        reels_ios_native_input_replace_text(id.as_ptr(), text.as_ptr());
    }
}

#[cfg(target_os = "ios")]
pub fn move_placeholder(id: &str, x: f32, y: f32) {
    let id = c_string(id);
    unsafe {
        reels_ios_native_input_move(id.as_ptr(), x, y);
    }
}

pub fn hide_active() {
    let id = {
        let mut state = state().lock().expect("native input state poisoned");
        state.editing = false;
        state.active_id.take()
    };
    let Some(id) = id else {
        return;
    };
    let id = c_string(&id);
    unsafe {
        reels_ios_native_input_end_editing(id.as_ptr());
    }
}

#[cfg(target_os = "ios")]
pub fn focus(id: &str) {
    let id = c_string(id);
    unsafe {
        reels_ios_native_input_focus(id.as_ptr());
    }
}

/// Let the native UITextView own the Paste action. Its delegate reports the
/// final text in the same run-loop turn, so the transparent slot grows with
/// the visible text instead of one frame later.
#[cfg(target_os = "ios")]
pub fn paste(id: &str) {
    let id = c_string(id);
    unsafe {
        reels_ios_native_input_paste(id.as_ptr());
    }
}

pub fn hide(id: &str) {
    {
        let mut state = state().lock().expect("native input state poisoned");
        if state.active_id.as_deref() == Some(id) {
            state.active_id = None;
            state.editing = false;
        }
    }
    let id = c_string(id);
    unsafe {
        reels_ios_native_input_hide(id.as_ptr());
    }
}

pub fn hide_all() {
    let ids = {
        let mut state = state().lock().expect("native input state poisoned");
        state.active_id = None;
        state.editing = false;
        state.events.clear();
        state.focus_events.clear();
        state.height_events.clear();
        state.text_by_id.keys().cloned().collect::<Vec<_>>()
    };
    for id in ids {
        let id = c_string(&id);
        unsafe {
            reels_ios_native_input_hide(id.as_ptr());
        }
    }
}

/// Forget pending native measurements for a field whose value or geometry
/// changed outside UIKit. Stale heights must not override the next Rust
/// layout after Paste, Clear or a dialog reset.
pub fn clear_height_events(id: &str) {
    let mut state = state().lock().expect("native input state poisoned");
    state.height_events.retain(|event| event.id.as_str() != id);
}

pub fn current_text() -> Option<String> {
    let state = state().lock().expect("native input state poisoned");
    state
        .active_id
        .as_ref()
        .and_then(|id| state.text_by_id.get(id))
        .cloned()
}

pub fn text_for(id: &str) -> Option<String> {
    state()
        .lock()
        .expect("native input state poisoned")
        .text_by_id
        .get(id)
        .cloned()
}

/// Replace the Rust-side authoritative text for a reusable native field.
pub fn set_text(id: &str, text: &str) {
    let mut state = state().lock().expect("native input state poisoned");
    state.text_by_id.insert(id.to_string(), text.to_string());
    state
        .pending_text_sync
        .insert(id.to_string(), text.to_string());
    state.events.retain(|event| event.id.as_str() != id);
    state.height_events.retain(|event| event.id.as_str() != id);
}

/// Confirm that the placeholder's next render delivered a Rust Paste/Clear
/// sync to UIKit. A later UIKit callback from the same gesture is stale.
#[cfg(target_os = "ios")]
pub fn confirm_native_text(id: &str, text: &str) {
    let mut state = state().lock().expect("native input state poisoned");
    state.text_by_id.insert(id.to_string(), text.to_string());
    state.events.retain(|event| event.id.as_str() != id);
    state.height_events.retain(|event| event.id.as_str() != id);
}

pub fn is_editing(id: &str) -> bool {
    let state = state().lock().expect("native input state poisoned");
    state.editing && state.active_id.as_deref() == Some(id)
}

pub fn drain_events() -> NativeInputEvents {
    let mut state = state().lock().expect("native input state poisoned");
    NativeInputEvents {
        text: state.events.drain(..).collect(),
        focus: state.focus_events.drain(..).collect(),
        height: state.height_events.drain(..).collect(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn reels_native_text_input_changed(
    id_pointer: *const c_char,
    text_pointer: *const c_char,
    selection_start: usize,
    selection_length: usize,
) {
    let Some(id) = (unsafe { read_c_string(id_pointer) }) else {
        return;
    };
    let Some(text) = (unsafe { read_c_string(text_pointer) }) else {
        return;
    };

    let changed = {
        let mut state = state().lock().expect("native input state poisoned");
        state
            .selection_by_id
            .insert(id.clone(), (selection_start, selection_length));
        let changed = state.text_by_id.get(&id) != Some(&text);
        if changed {
            state.text_by_id.insert(id.clone(), text.clone());
            state.events.push_back(TextEvent {
                id: id.clone(),
                text: text.clone(),
            });
        }
        changed
    };

    if changed {
        // `gpui_ios_request_frame` only forces a render when GPUI's own text
        // input reports a dirty edit. Native UITextView edits bypass that path,
        // so mark the same flag before Swift asks GPUI to render the queued
        // text event. Without it, the native text visibly changes while Rust
        // keeps stale button and layout state.
        gpui_ios::TEXT_INPUT_DIRTY.store(true, Ordering::Release);
        douyin_input::note_native_editor_text_echo(&id, &text);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn reels_native_text_input_selection_changed(
    id_pointer: *const c_char,
    selection_start: usize,
    selection_length: usize,
) {
    let Some(id) = (unsafe { read_c_string(id_pointer) }) else {
        return;
    };
    state()
        .lock()
        .expect("native input state poisoned")
        .selection_by_id
        .insert(id, (selection_start, selection_length));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn reels_native_text_input_height_changed(
    id_pointer: *const c_char,
    text_pointer: *const c_char,
    height: f32,
) {
    let Some(id) = (unsafe { read_c_string(id_pointer) }) else {
        return;
    };
    let Some(text) = (unsafe { read_c_string(text_pointer) }) else {
        return;
    };
    state()
        .lock()
        .expect("native input state poisoned")
        .height_events
        .push_back(HeightEvent { id, text, height });
    gpui_ios::TEXT_INPUT_DIRTY.store(true, Ordering::Release);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn reels_native_text_input_focus_changed(
    id_pointer: *const c_char,
    focused: i32,
) {
    let Some(id) = (unsafe { read_c_string(id_pointer) }) else {
        return;
    };
    {
        let mut state = state().lock().expect("native input state poisoned");
        if focused != 0 {
            state.active_id = Some(id.clone());
            state.editing = true;
        } else if state.active_id.as_deref() == Some(id.as_str()) {
            state.active_id = None;
            state.editing = false;
        }
        state
            .focus_events
            .retain(|event| event.id.as_str() != id.as_str());
        state.focus_events.push_back(FocusEvent {
            id,
            focused: focused != 0,
        });
    }

    gpui_ios::TEXT_INPUT_DIRTY.store(true, Ordering::Release);
    // This callback can run synchronously while UIKit is dismissing the
    // keyboard from inside a GPUI event. Requesting a frame here re-enters
    // GPUI's borrowed App state and panics with `RefCell already borrowed`.
    // Swift coalesces the focus update and requests the next frame
    // asynchronously after the UIKit callback returns.
}
