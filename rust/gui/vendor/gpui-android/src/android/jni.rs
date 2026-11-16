//! Android entry point and event loop using the `android-activity` crate.
//!
//! This module replaces the previous hand-rolled `ANativeActivity_onCreate`,
//! `JNI_OnLoad`, and lifecycle callback implementations with the higher-level
//! `android-activity` glue layer.
//!
//! ## Entry sequence
//!
//! ```text
//! android-activity loads the .so and calls android_main(app: AndroidApp)
//!   └── We store the AndroidApp globally
//!       └── Call the user-supplied gpui_android_main(app)
//! ```
//!
//! ## Threading model
//!
//! `android-activity` spawns a dedicated native thread and calls `android_main`
//! on it.  All GPUI draw / event callbacks run on this thread.  The
//! `AndroidApp` handle is `Send + Sync` and can be shared across threads.
//!
//! ## User entry point
//!
//! Applications must define:
//!
//! ```rust,no_run
//! #[no_mangle]
//! fn android_main(app: android_activity::AndroidApp) {
//!     // Initialise GPUI and run the application.
//! }
//! ```
//!
//! ## Event handling
//!
//! Lifecycle events (window creation/destruction, focus changes, etc.) are
//! delivered via `AndroidApp::poll_events()`.  Input events are obtained via
//! `AndroidApp::input_events_iter()`.

#![allow(unsafe_code)]
#![allow(non_snake_case)]

use std::{
    ffi::c_void,
    sync::{
        Arc, LazyLock, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use parking_lot::Mutex;

/// Whether the deferred init-window callback has already been invoked.
///
/// Reset to `false` on `TerminateWindow` so that when the surface is
/// recreated on resume the init callbacks run again.
static INIT_WINDOW_DONE: AtomicBool = AtomicBool::new(false);

/// Logical-pixel bounds `(x, y, w, h)` of the active native editor takeover.
///
/// Touches landing inside these bounds are routed to the real `EditText`
/// instead of GPUI so the system owns the caret, selection handles, and its
/// own edit menu.
pub(crate) static NATIVE_EDITOR_BOUNDS: LazyLock<Mutex<Option<(f32, f32, f32, f32)>>> =
    LazyLock::new(|| Mutex::new(None));

/// Whether the current touch gesture is being routed to the native editor.
static EDITOR_TOUCH_CLAIMED: AtomicBool = AtomicBool::new(false);

/// Record (or clear) the native editor takeover bounds, in logical pixels.
pub fn record_native_editor_bounds(bounds: Option<(f32, f32, f32, f32)>) {
    *NATIVE_EDITOR_BOUNDS.lock() = bounds;
}

/// Whether the GPUI native library has completed initialization.
///
/// Set to `true` after the first frame is rendered. This can be queried
/// via JNI by a custom Activity to dismiss the splash screen, although
/// with NativeActivity the system splash handles this automatically.
pub static NATIVE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Set after the renderer has submitted the first real GPUI frame.
///
/// `NATIVE_INITIALIZED` is published before the first frame is requested.
/// The Android host uses this second signal to keep its launch cover above
/// the NativeActivity surface until that surface contains app pixels.
pub static NATIVE_FIRST_FRAME_PRESENTED: AtomicBool = AtomicBool::new(false);

/// Editor window operation dispatched during paint, parked until the frame
/// that produced it has been submitted by `AndroidWindow::draw`.
///
/// Editor dispatches run in the middle of GPUI's paint phase, before the
/// frame is submitted. Applying them immediately lets the EditText window
/// move, attach, or detach while the screen still shows the previous frame —
/// most visibly as a lone input box floating over a black surface right
/// after the app resumes from a frozen background. Parking and replaying
/// right after `draw` keeps the editor window in lockstep with the frame
/// that defines its geometry.
enum PendingEditorOp {
    Show(crate::NativeEditorParams),
    Hide,
}
static PENDING_EDITOR_OP: Mutex<Option<PendingEditorOp>> = Mutex::new(None);

/// Set when Java finishes an IME animation during the Cookie dialog handoff.
/// The next submitted GPUI frame reveals the real editor at its final place.
static IME_ANIMATION_END_PENDING: AtomicBool = AtomicBool::new(false);
static NATIVE_EDITOR_IME_PREVIEW_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Set when the app resumes; the Java host shows a theme-colored cover until
/// the first resumed GPUI frame is submitted (see `apply_resume`).
pub static RESUME_COVER_PENDING: AtomicBool = AtomicBool::new(false);
/// True from resume until the cover fade starts. While set, editor ops stay
/// parked so the native editor panel never renders above the opaque cover
/// before the GPUI UI is revealed (Java flushes via
/// `nativeOnResumeCoverFadeStart`).
pub static RESUME_COVER_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Deferred lifecycle flags.
///
/// We must NOT call `win.set_active()` or `platform.did_enter_background()`
/// inside `handle_main_event` (which runs within `poll_events`).
/// The android-activity crate's Java-side callbacks block on a condvar
/// waiting for the native thread to finish processing the command.
/// If our handler tries to acquire the window `state` lock (a
/// `parking_lot::Mutex`), and a background thread is holding it (e.g.
/// during a render pass), we deadlock: native waits on the lock,
/// the lock holder waits for native rendering to complete, but native
/// is stuck.
///
/// Instead, the handlers set these flags and the main loop body
/// processes them AFTER `poll_events` returns.
static PAUSE_PENDING: AtomicBool = AtomicBool::new(false);
static RESUME_PENDING: AtomicBool = AtomicBool::new(false);
static TERM_WINDOW_PENDING: AtomicBool = AtomicBool::new(false);
static INIT_WINDOW_PENDING: AtomicBool = AtomicBool::new(false);
static WINDOW_RESIZED_PENDING: AtomicBool = AtomicBool::new(false);
static CONFIG_CHANGED_PENDING: AtomicBool = AtomicBool::new(false);

/// Interval at which a visible-but-idle app presents a keep-alive frame.
/// MIUI's dynamic-FPS controller stops compositing layers that stop
/// submitting buffers, and the first buffer release after that freeze can
/// stall `eglSwapBuffers` for hundreds of milliseconds — the perceived
/// 1-2 s click delay. Regular submissions keep the layer out of that state.
const FRAME_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(1);

use android_activity::{AndroidApp, MainEvent, PollEvent};

use super::platform::{AndroidPlatform, SharedPlatform};

use jni::JavaVM;
use jni::objects::{Global, JObject, JString, JValue};

// ── JNI helpers (safe `jni` crate wrappers) ──────────────────────────────────

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();

/// Get or create the static `JavaVM` wrapper.
fn java_vm_safe() -> Result<&'static JavaVM, String> {
    if let Some(vm) = JAVA_VM.get() {
        return Ok(vm);
    }
    let ptr = java_vm();
    if ptr.is_null() {
        return Err("JavaVM not available".into());
    }
    Ok(JAVA_VM.get_or_init(|| unsafe { JavaVM::from_raw(ptr as *mut jni::sys::JavaVM) }))
}

/// Run a closure with an attached `jni::Env` for the current thread.
///
/// In jni 0.22 the `attach_current_thread` API is closure-based.
/// The thread is auto-detached when the closure returns (if it was
/// not already attached).
pub fn with_env<T>(f: impl FnOnce(&mut jni::Env) -> Result<T, String>) -> Result<T, String> {
    let vm = java_vm_safe()?;
    // Return the closure result through attach's own return channel. A pending
    // Java exception (or a detach failure) makes attach return Err without a
    // closure result; smuggling it out via Option plus unwrap turned that
    // fallible boundary into a runner crash.
    vm.attach_current_thread(
        |env: &mut jni::Env| -> Result<Result<T, String>, jni::errors::Error> { Ok(f(env)) },
    )
    .map_err(|e: jni::errors::Error| e.to_string())?
}

/// Convenience alias: kept so existing callers that import `obtain_env`
/// compile with minimal changes. Returns a result by running the given
/// closure inside `with_env`.
#[inline]
pub fn obtain_env<T>(f: impl FnOnce(&mut jni::Env) -> Result<T, String>) -> Result<T, String> {
    with_env(f)
}

/// Get the Activity as a [`JObject`].
///
/// `activity_as_ptr()` returns a JNI global reference from `android-activity`
/// that is valid for the lifetime of the app. We wrap it in a `JObject`.
///
/// Requires `&Env` because jni 0.22's `JObject::from_raw` binds the
/// local-reference-frame lifetime.
pub fn activity<'local>(env: &jni::Env<'local>) -> Result<JObject<'local>, String> {
    let ptr = activity_as_ptr();
    if ptr.is_null() {
        return Err("Activity not available".into());
    }
    Ok(unsafe { JObject::from_raw(env, ptr as jni::sys::jobject) })
}

/// Convert a Java String (`JObject` wrapping a `java.lang.String`) to a Rust `String`.
///
/// Returns an empty string on null or error.
pub fn get_string(env: &mut jni::Env<'_>, obj: &JObject<'_>) -> String {
    if obj.is_null() {
        return String::new();
    }
    let jstr = unsafe { JString::from_raw(env, obj.as_raw()) };
    jstr.to_string()
}

/// Read a string extra from the Activity's launch Intent.
///
/// This is intended for app startup configuration. Missing extras and JNI
/// failures return `None`; the helper does not mutate process environment or
/// retain the Intent beyond the call.
pub fn launch_intent_string_extra(name: &str) -> Option<String> {
    with_env(|env| {
        // Platform-feature probes intentionally leave benign Java exceptions
        // pending on this thread. Clear them before this unrelated JNI call;
        // otherwise jni-rs refuses the call with JavaException.
        if env.exception_check() {
            env.exception_clear();
        }
        let activity = activity(env)?;
        let intent = env
            .call_method(
                &activity,
                jni::jni_str!("getIntent"),
                jni::jni_sig!("()Landroid/content/Intent;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if intent.is_null() {
            return Ok(None);
        }
        let key = env.new_string(name).map_err(|error| error.to_string())?;
        let value = env
            .call_method(
                &intent,
                jni::jni_str!("getStringExtra"),
                jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&key)],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(get_string(env, &value)))
        }
    })
    .ok()
    .flatten()
}

/// Extension trait for converting `jni::errors::Result<T>` to `Result<T, String>`.
pub(crate) trait JniExt<T> {
    fn e(self) -> Result<T, String>;
}

impl<T> JniExt<T> for jni::errors::Result<T> {
    fn e(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

/// Find an application class by name using the Activity's classloader.
///
/// From native threads, `JNIEnv::FindClass` uses the system classloader
/// which doesn't know about application classes.  This helper uses the
/// Activity's classloader via `activity.getClass().getClassLoader().loadClass(name)`.
///
/// `class_name` uses Java dot notation (e.g. `"dev.gpui.mobile.GpuiHelper"`).
pub fn find_app_class<'local>(
    env: &mut jni::Env<'local>,
    class_name: &str,
) -> Result<jni::objects::JClass<'local>, String> {
    let act = activity(env)?;

    // activity.getClassLoader() — call on the Context instance directly.
    // Do NOT use activity.getClass().getClassLoader(): NativeActivity is a
    // framework class loaded by BootClassLoader, which cannot see app classes.
    let class_loader = env
        .call_method(
            &act,
            jni::jni_str!("getClassLoader"),
            jni::jni_sig!("()Ljava/lang/ClassLoader;"),
            &[],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            env.exception_clear();
            let msg = format!("getClassLoader failed: {e}");
            log::error!("find_app_class({class_name}): {msg}");
            msg
        })?;

    // classLoader.loadClass("dev.gpui.mobile.GpuiHelper")
    let jname = env.new_string(class_name).e()?;
    let loaded = env
        .call_method(
            &class_loader,
            jni::jni_str!("loadClass"),
            jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
            &[JValue::Object(&jname)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            // Print full Java stack trace to logcat, then clear.
            env.exception_describe();
            env.exception_clear();
            let msg = format!("loadClass({class_name}) failed: {e}");
            log::error!("{msg}");
            msg
        })?;

    log::debug!("find_app_class: loaded {class_name}");
    Ok(unsafe { jni::objects::JClass::from_raw(env, loaded.as_raw()) })
}

// ── cached application-class lookup ─────────────────────────────────────────
//
// `find_app_class` goes through the Activity classloader (`loadClass`) on
// every call. Platform-view bounds/visibility updates and the lifecycle
// helpers call it repeatedly, so the resolved class is retained as a JNI
// global reference (same pattern as `KEY_CHARACTER_MAP` above and the IME
// cache in `window.rs`). A failed call through a cached class invalidates the
// entry so the next call transparently falls back to a fresh lookup.

/// Application classes resolved via the Activity classloader, by dot name.
///
/// The global references are owned by this map; readers only borrow the raw
/// pointer for the duration of one `Env` call.
static CACHED_APP_CLASSES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, Global<JObject<'static>>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Resolve an application class, reusing a cached global reference.
///
/// Falls back to [`find_app_class`] on a cache miss. Callers should invoke
/// [`invalidate_app_class_cache`] when a call through the returned class
/// fails, in case the retained reference went stale.
pub fn find_cached_app_class<'local>(
    env: &mut jni::Env<'local>,
    class_name: &str,
) -> Result<jni::objects::JClass<'local>, String> {
    if let Some(raw) = CACHED_APP_CLASSES
        .lock()
        .unwrap()
        .get(class_name)
        .map(|global| global.as_obj().as_raw())
    {
        if !raw.is_null() {
            return Ok(unsafe { jni::objects::JClass::from_raw(env, raw) });
        }
        CACHED_APP_CLASSES.lock().unwrap().remove(class_name);
    }
    let class = find_app_class(env, class_name)?;
    // Best-effort: retain a global reference for subsequent calls. `class`
    // converts into a plain `JObject` view; the global ref owns an
    // independent lifetime, so the locals may drop at frame pop as usual.
    let obj: JObject<'_> = class.into();
    let raw = obj.as_raw();
    if !raw.is_null()
        && let Ok(global) = env.new_global_ref(&obj)
    {
        CACHED_APP_CLASSES
            .lock()
            .unwrap()
            .insert(class_name.to_string(), global);
    }
    Ok(unsafe { jni::objects::JClass::from_raw(env, raw) })
}

/// Drop a cached application class, forcing a fresh lookup next time.
///
/// Call this when a JNI call through a cached class fails: the retained
/// reference may predate an Activity recreation.
pub fn invalidate_app_class_cache(class_name: &str) {
    CACHED_APP_CLASSES.lock().unwrap().remove(class_name);
}

// ── global state ─────────────────────────────────────────────────────────────

/// A replaceable process-global slot.
///
/// Android may destroy and recreate a `NativeActivity` without killing the
/// process.  Each recreated Activity gets a new `android_main` thread with a
/// new looper, so slot values tied to the previous Activity must be replaced
/// rather than initialized once for the lifetime of the process.
struct GlobalSlot<T>(Mutex<Option<T>>);

impl<T: Clone> GlobalSlot<T> {
    const fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn get(&self) -> Option<T> {
        self.0.lock().clone()
    }

    fn replace(&self, value: T) -> Option<T> {
        self.0.lock().replace(value)
    }
}

/// The `AndroidApp` handle for the current Activity instance.
static ANDROID_APP: GlobalSlot<AndroidApp> = GlobalSlot::new();

/// The `AndroidPlatform` for the current Activity instance.
///
/// Replaced when Android recreates the Activity; its dispatcher is bound to
/// that Activity's `android_main` thread and looper.
static PLATFORM: GlobalSlot<Arc<AndroidPlatform>> = GlobalSlot::new();

/// Immutable virtual-keyboard character map shared by hardware key events.
static KEY_CHARACTER_MAP: OnceLock<Global<JObject<'static>>> = OnceLock::new();

/// Get the unicode character produced by an Android key event via JNI.
///
/// Uses a retained `KeyCharacterMap`, avoiding a Java `KeyEvent` allocation on
/// every transition. Returns 0 on failure.
pub fn unicode_char_for_key_event(key_code: i32, _action: i32, meta_state: i32) -> u32 {
    with_env(|env| {
        if KEY_CHARACTER_MAP.get().is_none() {
            let local = match env.call_static_method(
                jni::jni_str!("android/view/KeyCharacterMap"),
                jni::jni_str!("load"),
                jni::jni_sig!("(I)Landroid/view/KeyCharacterMap;"),
                &[JValue::Int(-1)],
            ) {
                Ok(value) => value.l().map_err(|error| error.to_string())?,
                Err(error) => return Err(error.to_string()),
            };
            let global = env
                .new_global_ref(local)
                .map_err(|error| error.to_string())?;
            let _ = KEY_CHARACTER_MAP.set(global);
        }
        let Some(key_character_map) = KEY_CHARACTER_MAP.get() else {
            return Ok(0);
        };
        match env.call_method(
            key_character_map.as_obj(),
            jni::jni_str!("get"),
            jni::jni_sig!("(II)I"),
            &[JValue::Int(key_code), JValue::Int(meta_state)],
        ) {
            Ok(v) => {
                let c = v.i().unwrap_or(0);
                Ok(if c > 0 { c as u32 } else { 0 })
            }
            Err(_) => {
                env.exception_clear();
                Ok(0)
            }
        }
    })
    .unwrap_or(0)
}

// ── public accessors ──────────────────────────────────────────────────────────

/// Public accessor for the JavaVM pointer.
///
/// Uses `AndroidApp::vm_as_ptr()` from the stored `AndroidApp`.
/// Used by `platform.rs` for JNI calls.
pub fn java_vm() -> *mut c_void {
    android_app()
        .map(|app| app.vm_as_ptr())
        .unwrap_or(std::ptr::null_mut())
}

/// Public accessor for the current Activity's JNI object reference.
///
/// Uses `AndroidApp::activity_as_ptr()` from the stored `AndroidApp`.
/// Used by `platform.rs` and `window.rs` for JNI calls that require the
/// activity's jobject.
///
/// NOTE: This returns a jobject (JNI global ref), NOT an `ANativeActivity *`.
/// Code that previously used `(*activity).clazz` should use this directly
/// as the activity jobject.
pub fn activity_as_ptr() -> *mut c_void {
    android_app()
        .map(|app| app.activity_as_ptr())
        .unwrap_or(std::ptr::null_mut())
}

/// Returns a clone of the stored `AndroidApp`, if initialised.
pub fn android_app() -> Option<AndroidApp> {
    ANDROID_APP.get()
}

/// Returns the current Activity's `AndroidPlatform`, if initialised.
///
/// Returns `None` before `android_main` has set it up.
pub fn platform() -> Option<Arc<AndroidPlatform>> {
    PLATFORM.get()
}

/// Returns a [`SharedPlatform`] wrapping the global `Arc<AndroidPlatform>`.
///
/// This is the value you hand to `Application::with_platform(...)`:
///
/// ```rust,no_run
/// let platform = jni::shared_platform().unwrap();
/// Application::with_platform(platform.into_rc()).run(|cx| { … });
/// ```
///
/// Returns `None` before `init_platform` has been called.
pub fn shared_platform() -> Option<SharedPlatform> {
    platform().map(SharedPlatform::new)
}

// ── input event types ─────────────────────────────────────────────────────────

// ── night mode query via NDK Configuration ───────────────────────────────────

/// Query the current night mode using the NDK Configuration API.
///
/// Returns `true` if the system is in dark mode.
pub fn query_night_mode_via_jni() -> bool {
    let via_system_resources = with_env(|env| {
        let resources_class = env
            .find_class(jni::jni_str!("android/content/res/Resources"))
            .e()?;
        let resources = env
            .call_static_method(
                &resources_class,
                jni::jni_str!("getSystem"),
                jni::jni_sig!("()Landroid/content/res/Resources;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if resources.is_null() {
            return Err("system Resources not available".into());
        }
        let configuration = env
            .call_method(
                &resources,
                jni::jni_str!("getConfiguration"),
                jni::jni_sig!("()Landroid/content/res/Configuration;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if configuration.is_null() {
            return Err("system Configuration not available".into());
        }
        let ui_mode = env
            .get_field(&configuration, jni::jni_str!("uiMode"), jni::jni_sig!("I"))
            .and_then(|value| value.i())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        Ok(ui_mode & 0x30 == 0x20)
    });
    if let Ok(is_dark) = via_system_resources {
        log::debug!("query_night_mode (system resources): is_dark={}", is_dark);
        return is_dark;
    }

    let app = match android_app() {
        Some(app) => app,
        None => return false,
    };

    // Build an ndk::configuration::Configuration from the app's asset manager.
    let config = ndk::configuration::Configuration::from_asset_manager(&app.asset_manager());
    let is_dark = config.ui_mode_night() == ndk::configuration::UiModeNight::Yes;

    log::debug!("query_night_mode (ndk): is_dark={}", is_dark);
    is_dark
}

fn query_display_scale_factor_via_jni() -> Option<f32> {
    let scale = with_env(|env| {
        let activity = activity(env)?;
        let resources = env
            .call_method(
                &activity,
                jni::jni_str!("getResources"),
                jni::jni_sig!("()Landroid/content/res/Resources;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if resources.is_null() {
            return Err("Activity Resources not available".into());
        }
        let metrics = env
            .call_method(
                &resources,
                jni::jni_str!("getDisplayMetrics"),
                jni::jni_sig!("()Landroid/util/DisplayMetrics;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if metrics.is_null() {
            return Err("Activity DisplayMetrics not available".into());
        }
        env.get_field(&metrics, jni::jni_str!("density"), jni::jni_sig!("F"))
            .and_then(|value| value.f())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })
    });
    match scale {
        Ok(scale) if scale.is_finite() && scale > 0.0 => Some(scale),
        Ok(_) => None,
        Err(error) => {
            log::warn!("query_display_scale_factor failed: {error}");
            None
        }
    }
}

// ── input event processing ────────────────────────────────────────────────────

/// Map an NDK motion action onto the testable [`crate::event_stages::MotionKind`].
///
/// This is the **decode** stage of the input pipeline: everything downstream
/// (`decode_motion_slot`) is pure and unit-tested on any host.
fn motion_kind(action: android_activity::input::MotionAction) -> crate::event_stages::MotionKind {
    use crate::event_stages::MotionKind as Kind;
    use android_activity::input::MotionAction as Native;
    match action {
        Native::Down => Kind::Down,
        Native::PointerDown => Kind::PointerDown,
        Native::Up => Kind::Up,
        Native::PointerUp => Kind::PointerUp,
        Native::Move => Kind::Move,
        Native::Cancel => Kind::Cancel,
        _ => Kind::Other,
    }
}

fn finite_pressure(value: f32) -> Option<f32> {
    value.is_finite().then(|| value.clamp(0.0, 1.0))
}

/// Process input events from the `AndroidApp` and dispatch them to the window.
fn process_input_events(app: &AndroidApp) {
    let platform = match PLATFORM.get() {
        Some(p) => p,
        None => {
            log::trace!("process_input_events: no platform yet");
            return;
        }
    };

    let win = match platform.primary_window() {
        Some(w) => w,
        None => {
            log::trace!("process_input_events: no primary window yet");
            return;
        }
    };

    match app.input_events_iter() {
        Ok(mut iter) => {
            loop {
                let read_input = iter.next(|event| {
                    use android_activity::input::{InputEvent, MotionAction};

                    match event {
                        InputEvent::MotionEvent(motion_event) => {
                            let action = motion_event.action();
                            let pointer_count = motion_event.pointer_count();

                            // Native editor takeover: let Android's view
                            // hierarchy dispatch the real MotionEvent to the
                            // EditText. Synthetic re-dispatch bypasses the
                            // ViewRoot timing that drives native selection and
                            // ActionMode, so avoid fabricating events here.
                            {
                                let claimed =
                                    EDITOR_TOUCH_CLAIMED.load(Ordering::Acquire);
                                let is_hand_action = matches!(
                                    action,
                                    MotionAction::Down
                                        | MotionAction::Up
                                        | MotionAction::Move
                                        | MotionAction::Cancel
                                        | MotionAction::PointerDown
                                        | MotionAction::PointerUp
                                );
                                if is_hand_action {
                                    let claimed_now = if action == MotionAction::Down {
                                        let in_bounds = {
                                            let bounds_opt =
                                                *NATIVE_EDITOR_BOUNDS.lock();
                                            bounds_opt
                                                .map(|(bx, by, bw, bh)| {
                                                    let scale =
                                                        win.scale_factor();
                                                    let primary = motion_event
                                                        .pointer_at_index(0);
                                                    let logical_x =
                                                        primary.x() / scale;
                                                    let logical_y =
                                                        primary.y() / scale;
                                                    logical_x >= bx
                                                        && logical_x < bx + bw
                                                        && logical_y >= by
                                                        && logical_y < by + bh
                                                })
                                                .unwrap_or(false)
                                        };
                                        EDITOR_TOUCH_CLAIMED.store(
                                            in_bounds,
                                            Ordering::Release,
                                        );
                                        in_bounds
                                    } else {
                                        claimed
                                    };
                                    if claimed_now {
                                        if matches!(
                                            action,
                                            MotionAction::Up
                                                | MotionAction::Cancel
                                        ) {
                                            EDITOR_TOUCH_CLAIMED.store(
                                                false,
                                                Ordering::Release,
                                            );
                                        }
                                        return android_activity::InputStatus::Unhandled;
                                    }
                                }
                            }

                            log::info!(
                                "process_input_events: MotionEvent action={:?} pointers={}",
                                action,
                                pointer_count,
                            );

                            // Check if this touch lands on a platform view.
                            //
                            // On Android with NativeActivity, ALL touch events go
                            // to the native surface first. Platform views are real
                            // Java Views in a FrameLayout overlay, but they won't
                            // receive touches unless we skip GPUI dispatch and let
                            // Android's view hierarchy handle the event instead.
                            //
                            // We use the primary pointer for the hit-test on DOWN
                            // actions. Physical pixel coordinates are converted to
                            // logical pixels to match PlatformViewBounds.
                            let hits_platform_view = {
                                let registry = crate::platform_view::PlatformViewRegistry::global();
                                if registry.active_view_count() > 0 {
                                    let primary = motion_event.pointer_at_index(
                                        match action {
                                            MotionAction::PointerDown | MotionAction::PointerUp => {
                                                motion_event.pointer_index()
                                            }
                                            _ => 0,
                                        }
                                    );
                                    let scale = win.scale_factor();
                                    let logical_x = primary.x() / scale;
                                    let logical_y = primary.y() / scale;
                                    registry.hit_test(logical_x, logical_y)
                                } else {
                                    false
                                }
                            };

                            if hits_platform_view {
                                log::debug!(
                                    "process_input_events: touch hits platform view, skipping GPUI dispatch",
                                );
                                // Return Unhandled so android-activity can pass
                                // the event back to the Java view hierarchy where
                                // the platform view's FrameLayout lives.
                                return android_activity::InputStatus::Unhandled;
                            }

                            // Decode stage: one masked action per pointer slot.
                            // Hover/scroll/button actions decode to `None` and
                            // are skipped so the OS can route them elsewhere.
                            let kind = motion_kind(action);
                            let pointer_index = motion_event.pointer_index();
                            for i in 0..pointer_count {
                                let Some(touch_action) =
                                    crate::event_stages::decode_motion_slot(
                                        kind,
                                        pointer_index,
                                        i,
                                    )
                                else {
                                    continue;
                                };
                                let pointer = motion_event.pointer_at_index(i);

                                let history = pointer
                                    .history()
                                    .map(|sample| crate::android::HistoricalTouchPoint {
                                        timestamp_ns: sample.event_time().max(0) as u64,
                                        x: sample.x(),
                                        y: sample.y(),
                                        pressure: finite_pressure(sample.pressure()),
                                    })
                                    .collect();
                                let tool = match pointer.tool_type() {
                                    android_activity::input::ToolType::Finger => {
                                        crate::android::TouchTool::Finger
                                    }
                                    android_activity::input::ToolType::Stylus
                                    | android_activity::input::ToolType::Eraser => {
                                        crate::android::TouchTool::Stylus
                                    }
                                    _ => crate::android::TouchTool::Other,
                                };

                                let touch = crate::android::TouchPoint {
                                    id: pointer.pointer_id(),
                                    x: pointer.x(),
                                    y: pointer.y(),
                                    action: touch_action,
                                    timestamp_ns: motion_event.event_time().max(0) as u64,
                                    tool,
                                    pressure: finite_pressure(pointer.pressure()),
                                    history,
                                };

                                log::debug!(
                                    "process_input_events: dispatching touch id={} x={:.0} y={:.0} action={}",
                                    touch.id, touch.x, touch.y, touch.action,
                                );

                                win.handle_touch(touch);
                            }

                            android_activity::InputStatus::Handled
                        }
                        InputEvent::KeyEvent(key_event) => {
                            use android_activity::input::KeyAction;
                            use crate::event_stages::{KeyDirection, decode_key_action};

                            let direction = match key_event.action() {
                                KeyAction::Down => KeyDirection::Press,
                                KeyAction::Up => KeyDirection::Release,
                                _ => return android_activity::InputStatus::Unhandled,
                            };
                            // Single source of truth for the ACTION_DOWN = 0 /
                            // ACTION_UP = 1 encoding stored in AndroidKeyEvent.
                            let action = direction.action_int();
                            debug_assert_eq!(decode_key_action(action), Some(direction));

                            let key_code: u32 = key_event.key_code().into();
                            if key_code as i32 == crate::android::AKEYCODE_BACK {
                                if matches!(direction, KeyDirection::Press) {
                                    crate::dispatch_back_handler();
                                }
                                return android_activity::InputStatus::Handled;
                            }
                            let meta_state: u32 = key_event.meta_state().0;

                            if crate::native_editor_active() {
                                dispatch_native_editor_key_android(
                                    action,
                                    key_code as i32,
                                    meta_state as i32,
                                );
                                return android_activity::InputStatus::Handled;
                            }

                            let unicode_char = unicode_char_for_key_event(
                                key_code as i32,
                                action,
                                meta_state as i32,
                            );

                            if unicode_char != 0 {
                                log::trace!(
                                    "dispatch_key_event: code={} action={} meta={:#x} → unicode=U+{:04X}",
                                    key_code,
                                    action,
                                    meta_state,
                                    unicode_char
                                );
                            }

                            let key_event = crate::android::AndroidKeyEvent {
                                key_code: key_code as i32,
                                action,
                                meta_state: meta_state as i32,
                                unicode_char,
                            };

                            win.handle_key_event(key_event);
                            android_activity::InputStatus::Handled
                        }
                        _ => android_activity::InputStatus::Unhandled,
                    }
                });

                if !read_input {
                    break;
                }
            }
        }
        Err(err) => {
            log::error!("Failed to get input events iterator: {err:?}");
        }
    }
}

// ── main event loop ───────────────────────────────────────────────────────────

fn event_loop_poll_timeout(
    needs_frame_pump: bool,
    next_delayed_task: Option<Duration>,
) -> Option<Duration> {
    // Tested on every host in `crate::event_stages`; kept as a thin alias so
    // the loop reads in domain terms.
    crate::event_stages::frame_poll_timeout(needs_frame_pump, next_delayed_task)
}

/// Drain lifecycle events queued while a deferred handler ran.
///
/// Java UI-thread callbacks block on a condvar until the native thread drains
/// the looper, so each deferred stage re-polls with a zero timeout before
/// returning. This keeps the condvar wait short and prevents the
/// InputDispatcher ANR (10 s timeout on MotionEvent delivery).
fn drain_main_events(app: &AndroidApp) {
    app.poll_events(Some(Duration::ZERO), |event| {
        if let PollEvent::Main(main_event) = event {
            handle_main_event(app, main_event);
        }
    });
}

/// Deferred `TerminateWindow` stage: drop the renderer/surface, keep the
/// window struct (and its GPUI callbacks) alive for the next `InitWindow`.
fn apply_term_window(iteration: u64) {
    log::info!("deferred: TerminateWindow (iter={})", iteration);
    INIT_WINDOW_DONE.store(false, Ordering::Relaxed);
    *LAST_CHROME_STYLE.lock().unwrap() = None;
    if let Some(platform) = PLATFORM.get() {
        if let Some(win) = platform.primary_window() {
            win.term_window();
        }
    }
}

/// Apply the current system-bar/cutout safe area to a freshly opened window.
///
/// Activity recreation (for example after a system font change) can deliver
/// `WindowInsets` before the new GPUI window exists. That Java-side event is
/// then dropped, and the content rect fallback below reports a zero top inset.
/// Re-read the live insets here, after the window has been created, so the
/// first rendered frame already starts below the status bar/cutout.
fn update_safe_area_after_window_init(
    app: &AndroidApp,
    window: &crate::android::window::AndroidWindow,
) {
    match with_env(window_system_bar_insets) {
        Ok((left, top, right, bottom)) => {
            window.update_safe_area_insets(left, top, right, bottom);
        }
        Err(error) => {
            log::debug!("WindowInsets unavailable after window init: {error}");
            let content_rect = app.content_rect();
            window.update_safe_area_from_content_rect(
                content_rect.left,
                content_rect.top,
                content_rect.right,
                content_rect.bottom,
            );
        }
    }
}

/// Deferred `InitWindow` stage: bind the new surface onto the existing
/// renderer, or open the primary window when there is none yet.
fn apply_init_window(app: &AndroidApp, attempted_existing_window_init: &mut bool, iteration: u64) {
    log::info!("deferred: InitWindow (iter={})", iteration);
    *attempted_existing_window_init = true;
    if let Some(platform) = PLATFORM.get() {
        if let Some(native_window) = app.native_window() {
            let width = native_window.width();
            let height = native_window.height();
            log::info!("InitWindow: {}×{}", width, height);

            platform.update_primary_display(&native_window, &app.asset_manager());

            let scale_factor = query_display_scale_factor_via_jni()
                .or_else(|| platform.primary_display().map(|d| d.scale_factor()))
                .unwrap_or(1.0);
            platform.update_primary_display_scale(scale_factor);

            if let Some(existing) = platform.primary_window() {
                let gpu_ctx = platform.gpu_context();
                match existing.init_window(native_window, gpu_ctx) {
                    Ok(()) => {
                        log::info!("InitWindow: reinitialised existing window");
                        existing.set_active(true);
                        existing.set_appearance(current_window_appearance());
                        existing.request_frame();
                    }
                    Err(e) => {
                        log::error!("failed to reinit window surface: {e:#}");
                    }
                }

                // Trigger GPUI resize so layout adapts to new dimensions.
                existing.update_scale_factor(scale_factor);
                existing.handle_resize();

                update_safe_area_after_window_init(app, &existing);

                INIT_WINDOW_DONE.store(true, Ordering::Relaxed);
                existing.request_frame();
            } else {
                match platform.open_window(native_window, scale_factor, false) {
                    Ok(win) => {
                        log::info!(
                            "window opened — id={:#x} scale={:.1}",
                            win.id(),
                            scale_factor
                        );
                        win.set_active(true);
                        win.set_appearance(current_window_appearance());

                        update_safe_area_after_window_init(app, &win);
                    }
                    Err(e) => {
                        log::error!("failed to open window: {e:#}");
                    }
                }
            }
        }
    }
}

/// Deferred `WindowResized` stage: reconfigure the surface and safe area.
fn apply_window_resized(app: &AndroidApp) {
    log::debug!("deferred: WindowResized");
    if let Some(platform) = PLATFORM.get() {
        if let Some(win) = platform.primary_window() {
            win.handle_resize();
            let cr = app.content_rect();
            win.update_safe_area_from_content_rect(cr.left, cr.top, cr.right, cr.bottom);
            win.request_frame();
        }
    }
}

/// Deferred `ConfigChanged` stage: refresh keyboard layout and dark-mode.
fn apply_config_changed() {
    log::debug!("deferred: ConfigChanged");
    if let Some(platform) = PLATFORM.get() {
        platform.notify_keyboard_layout_change();
        if let Some(scale_factor) = query_display_scale_factor_via_jni() {
            platform.update_primary_display_scale(scale_factor);
            if let Some(win) = platform.primary_window() {
                win.update_scale_factor(scale_factor);
            }
        }
        if let Some(win) = platform.primary_window() {
            win.set_appearance(current_window_appearance());
        }
        if let Some(win) = platform.primary_window() {
            win.request_frame();
        }
    }
}

fn current_window_appearance() -> crate::android::window::WindowAppearance {
    if query_night_mode_via_jni() {
        crate::android::window::WindowAppearance::Dark
    } else {
        crate::android::window::WindowAppearance::Light
    }
}

/// Deferred `Pause` stage: mark backgrounded and pause platform views.
fn apply_pause(iteration: u64) {
    log::info!("deferred: Pause (iter={})", iteration);
    if let Some(platform) = PLATFORM.get() {
        platform.did_enter_background();
        if let Some(win) = platform.primary_window() {
            win.set_active(false);
        }
    }
    // Pause platform views
    pause_platform_views();
}

/// Deferred `Resume` stage: mark foregrounded and resume platform views.
fn apply_resume(iteration: u64) {
    log::info!("deferred: Resume (iter={})", iteration);
    // The surface is recreated empty on every resume (the system drops the
    // buffers at onStop), so the cover must always fill the gap between the
    // task snapshot dismissing and GPUI's first post-resume frame.
    RESUME_COVER_PENDING.store(true, Ordering::Release);
    RESUME_COVER_ACTIVE.store(true, Ordering::Release);
    if let Some(platform) = PLATFORM.get() {
        platform.did_become_active();
        if let Some(win) = platform.primary_window() {
            win.set_active(true);
            win.request_frame();
        }
    }
    // Resume platform views
    resume_platform_views();
}

/// Sync the loop's view of foreground state with the window.
fn track_active_state(app_is_active: &mut bool, iteration: u64) {
    if let Some(platform) = PLATFORM.get() {
        if let Some(win) = platform.primary_window() {
            let is_active = win.is_active();
            if is_active != *app_is_active {
                log::info!(
                    "run_event_loop: active {} -> {} (iter={})",
                    *app_is_active,
                    is_active,
                    iteration,
                );
                *app_is_active = is_active;
            }
        }
    }
}

/// Run the one-shot finish-launching / init-window callbacks, then pump the
/// first frame immediately so the surface never idles on black.
fn maybe_run_init_callbacks(iteration: u64) {
    if INIT_WINDOW_DONE.load(Ordering::Relaxed) {
        return;
    }
    let Some(platform) = PLATFORM.get() else {
        return;
    };
    if platform.primary_window().is_none() {
        return;
    }
    if let Some(finish_cb) = platform.take_finish_launching_callback() {
        log::info!("invoking finish_launching callback (iter={})", iteration);
        finish_cb();
        if let Some(win) = platform.primary_window() {
            win.refresh_appearance();
        }
    }

    if let Some(init_cb) = platform.take_on_init_window_callback() {
        let win = platform.primary_window().unwrap();
        log::info!("invoking on_init_window callback (iter={})", iteration);
        init_cb(win);
    }

    INIT_WINDOW_DONE.store(true, Ordering::Relaxed);
    NATIVE_INITIALIZED.store(true, Ordering::Release);
    log::info!("NATIVE_INITIALIZED = true");

    // Render first frame immediately.
    platform.flush_main_thread_tasks();
    if let Some(win) = platform.primary_window() {
        win.request_frame();
    }
}

/// Render stage: pump one GPUI frame when a wake source fired.
///
/// The gate decision itself (`should_pump_frame`) is pure and unit-tested in
/// `crate::event_stages`; this function only performs the effects. Note that
/// `take_main_thread_wake` is consumed every iteration even when no frame is
/// pumped, so a dispatched task is never lost.
fn pump_frame_if_needed(
    app: &AndroidApp,
    force_frame_after_poll: bool,
    platform_event_woke_frame: bool,
    needs_frame_pump: bool,
    app_is_active: bool,
) {
    let Some(platform) = PLATFORM.get() else {
        return;
    };
    let main_wake = platform.take_main_thread_wake();
    let keepalive_due = INIT_WINDOW_DONE.load(Ordering::Relaxed)
        && app_is_active
        && platform
            .primary_window()
            .and_then(|win| win.time_since_last_draw())
            .is_some_and(|elapsed| elapsed >= FRAME_KEEPALIVE_INTERVAL);
    let will_pump = keepalive_due
        || crate::event_stages::should_pump_frame(
            INIT_WINDOW_DONE.load(Ordering::Relaxed),
            app_is_active,
            force_frame_after_poll,
            platform_event_woke_frame,
            main_wake,
            needs_frame_pump,
        );
    log::debug!(
        "pump_frame_if_needed: pump={} keepalive={} force={} event_woke={} main_wake={} needs_pump={}",
        will_pump,
        keepalive_due,
        force_frame_after_poll,
        platform_event_woke_frame,
        main_wake,
        needs_frame_pump,
    );
    if !will_pump {
        return;
    }
    platform.flush_main_thread_tasks();
    if let Some(win) = platform.primary_window() {
        win.request_frame();
    }

    // Drain lifecycle events that arrived during rendering
    // (e.g. rotation triggers TerminateWindow while we were
    // in get_current_texture / present).
    drain_main_events(app);
    process_input_events(app);
}

/// The event loop that processes `android-activity` events and drives the
/// platform.
///
/// Called from `android_main` after the platform is initialised.
/// Runs until the platform requests quit or the activity is destroyed.
/// Keep momentum scrolling responsive, but otherwise let the Android looper
/// sleep until a platform event, foreground-task wake, or delayed task is due.
///
/// The loop is staged as **poll → decode → dispatch → draw**: lifecycle work
/// is deferred to the `apply_*` stage handlers, input mapping goes through
/// the tested `crate::event_stages` decoders, and frames pump through
/// `pump_frame_if_needed`.
pub fn run_event_loop(app: &AndroidApp) {
    log::info!("run_event_loop: entering main loop");

    // Track whether the on_init_window callback has already been invoked.
    // We do NOT invoke it inside handle_main_event (which runs inside
    // poll_events) because the callback can be heavy (shader compilation,
    // GPUI Application setup).  Running it there blocks the event loop and
    // prevents the system's FocusEvent from being consumed, triggering an
    // ANR after 10 seconds.
    //
    // Instead we check each loop iteration: if a primary window exists and
    // the callback is still pending, invoke it *after* poll_events has
    // returned so focus/input events have already been drained.
    INIT_WINDOW_DONE.store(false, Ordering::Relaxed);
    let mut iteration: u64 = 0;
    let mut last_heartbeat = std::time::Instant::now();
    let mut app_is_active = false;
    let mut attempted_existing_window_init = false;

    loop {
        iteration += 1;

        // Log a heartbeat every 5 seconds so we can tell if the loop is alive.
        let now = std::time::Instant::now();
        if now.duration_since(last_heartbeat) >= Duration::from_secs(5) {
            log::info!(
                "run_event_loop: heartbeat — iteration={}, init_done={}, active={}",
                iteration,
                INIT_WINDOW_DONE.load(Ordering::Relaxed),
                app_is_active,
            );
            last_heartbeat = now;
        }

        // Check if quit was requested.
        if let Some(platform) = PLATFORM.get() {
            if platform.should_quit() {
                platform.run_pending_quit_callback();
                log::info!("run_event_loop: platform requested quit");
                dispose_all_platform_views();
                break;
            }
            platform.tick();
        }

        let (poll_timeout, force_frame_after_poll, needs_frame_pump) = PLATFORM
            .get()
            .map(|platform| {
                let needs_frame_pump = app_is_active
                    && platform
                        .primary_window()
                        .is_some_and(|window| window.needs_frame_pump());
                let poll_timeout = event_loop_poll_timeout(
                    needs_frame_pump,
                    platform.next_delayed_task_in(Instant::now()),
                );
                // Wake the loop when the next keep-alive frame is due so an
                // idle-but-visible app keeps submitting buffers (see
                // `pump_frame_if_needed` for why).
                let keepalive_wait = (app_is_active && INIT_WINDOW_DONE.load(Ordering::Relaxed))
                    .then(|| {
                        platform
                            .primary_window()
                            .and_then(|win| win.time_since_last_draw())
                            .map(|elapsed| FRAME_KEEPALIVE_INTERVAL.saturating_sub(elapsed))
                    })
                    .flatten();
                let poll_timeout = match (poll_timeout, keepalive_wait) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                (
                    poll_timeout,
                    !INIT_WINDOW_DONE.load(Ordering::Relaxed),
                    needs_frame_pump,
                )
            })
            .unwrap_or((None, false, false));
        let mut platform_event_woke_frame = false;

        // ── Poll for events (non-blocking) ──
        //
        // Non-blocking poll: process any pending events then immediately
        // continue to rendering. No sleep — the GPU present call
        // (get_current_texture / Mailbox) provides natural frame pacing.
        app.poll_events(poll_timeout, |event| {
            platform_event_woke_frame |= !matches!(&event, PollEvent::Timeout);
            match event {
                PollEvent::Main(main_event) => {
                    if matches!(main_event, android_activity::MainEvent::InputAvailable) {
                        log::info!("poll_events: InputAvailable");
                    }
                    handle_main_event(app, main_event);
                }
                PollEvent::Wake => {}
                PollEvent::Timeout => {}
                _ => {}
            }
        });

        // Some NativeActivity launches can enter `android_main` after the
        // system has already installed the native window, without delivering a
        // fresh InitWindow event to this loop.  Treat a visible native window
        // as an init signal so GPUI can bind a renderer instead of idling on a
        // black surface forever.
        if !attempted_existing_window_init
            && !INIT_WINDOW_DONE.load(Ordering::Relaxed)
            && !INIT_WINDOW_PENDING.load(Ordering::Relaxed)
            && app.native_window().is_some()
        {
            log::info!("run_event_loop: synthesizing InitWindow from existing native window");
            INIT_WINDOW_PENDING.store(true, Ordering::Relaxed);
            attempted_existing_window_init = true;
        }

        // ── Deferred lifecycle processing ──
        //
        // Between stages the loop re-polls with a zero timeout
        // (`drain_main_events`) to drain events the Java UI thread queued
        // while the stage ran. This keeps the condvar wait short and prevents
        // the InputDispatcher ANR (10s timeout on MotionEvent delivery).

        // 1. TerminateWindow — unconfigure surface, release native window
        if TERM_WINDOW_PENDING.swap(false, Ordering::Relaxed) {
            apply_term_window(iteration);
            // Drain immediately — Java thread may be blocked on InitWindow condvar.
            drain_main_events(app);
        }

        // 2. InitWindow — replace surface on existing renderer, or create new
        if INIT_WINDOW_PENDING.swap(false, Ordering::Relaxed) {
            apply_init_window(app, &mut attempted_existing_window_init, iteration);
            // Drain — Java thread may have queued ConfigChanged/WindowResized.
            drain_main_events(app);
        }

        // 3. WindowResized
        if WINDOW_RESIZED_PENDING.swap(false, Ordering::Relaxed) {
            apply_window_resized(app);
        }

        // 4. ConfigChanged
        if CONFIG_CHANGED_PENDING.swap(false, Ordering::Relaxed) {
            apply_config_changed();
        }

        // 5. Pause / background
        if PAUSE_PENDING.swap(false, Ordering::Relaxed) {
            apply_pause(iteration);
        }

        // 6. Resume / foreground
        if RESUME_PENDING.swap(false, Ordering::Relaxed) {
            apply_resume(iteration);
        }

        // Track active/focused state.
        track_active_state(&mut app_is_active, iteration);

        // Process input events.
        process_input_events(app);

        // Deferred initialisation callbacks (runs once).
        maybe_run_init_callbacks(iteration);

        // ── Render ──
        pump_frame_if_needed(
            app,
            force_frame_after_poll,
            platform_event_woke_frame,
            needs_frame_pump,
            app_is_active,
        );
    }

    log::info!("run_event_loop: exiting main loop");
}

/// Pause all platform views when the app goes to background.
fn pause_platform_views() {
    let _ = with_env(|env| {
        if let Ok(helper_class) = find_cached_app_class(env, "dev.gpui.mobile.GpuiPlatformView") {
            let _ = env.call_static_method(
                &helper_class,
                jni::jni_str!("pauseAll"),
                jni::jni_sig!("()V"),
                &[],
            );
            env.exception_clear();
        }
        Ok(())
    });
}

/// Resume all platform views when the app returns to foreground.
fn resume_platform_views() {
    let _ = with_env(|env| {
        if let Ok(helper_class) = find_cached_app_class(env, "dev.gpui.mobile.GpuiPlatformView") {
            let _ = env.call_static_method(
                &helper_class,
                jni::jni_str!("resumeAll"),
                jni::jni_sig!("()V"),
                &[],
            );
            env.exception_clear();
        }
        Ok(())
    });
}

/// Dispose all platform views during app shutdown.
fn dispose_all_platform_views() {
    let _ = with_env(|env| {
        if let Ok(helper_class) = find_cached_app_class(env, "dev.gpui.mobile.GpuiPlatformView") {
            let _ = env.call_static_method(
                &helper_class,
                jni::jni_str!("disposeAll"),
                jni::jni_sig!("()V"),
                &[],
            );
            env.exception_clear();
        }
        Ok(())
    });
}

/// Handle a single `MainEvent` from `android-activity`.
fn handle_main_event(_app: &AndroidApp, event: MainEvent<'_>) {
    match event {
        MainEvent::InitWindow { .. } => {
            log::info!("MainEvent::InitWindow");
            // Defer to after poll_events to avoid deadlock with state lock.
            INIT_WINDOW_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::TerminateWindow { .. } => {
            log::info!("MainEvent::TerminateWindow");
            // Defer to after poll_events to avoid deadlock with state lock.
            TERM_WINDOW_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::WindowResized { .. } => {
            log::debug!("MainEvent::WindowResized");
            // Defer to after poll_events to avoid deadlock with state lock.
            WINDOW_RESIZED_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::GainedFocus => {
            log::info!("MainEvent::GainedFocus");
            RESUME_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::LostFocus => {
            if crate::native_editor_owns_window_focus() {
                log::info!("MainEvent::LostFocus — owned by native editor");
                return;
            }
            log::info!("MainEvent::LostFocus");
            PAUSE_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Resume { .. } => {
            log::info!("MainEvent::Resume");
            RESUME_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Pause => {
            log::info!("MainEvent::Pause");
            // set_active uses AtomicBool so it never blocks.
            PAUSE_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::ConfigChanged { .. } => {
            log::debug!("MainEvent::ConfigChanged");
            CONFIG_CHANGED_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::Start => {
            log::info!("MainEvent::Start");
        }

        MainEvent::Stop => {
            log::info!("MainEvent::Stop");
        }

        MainEvent::SaveState { .. } => {
            log::info!("MainEvent::SaveState");
        }

        MainEvent::LowMemory => {
            log::warn!("MainEvent::LowMemory — consider releasing cached resources");
        }

        MainEvent::Destroy => {
            log::info!("MainEvent::Destroy");

            if let Some(platform) = PLATFORM.get() {
                platform.quit();
            }
        }

        MainEvent::InsetsChanged { .. } => {
            log::debug!("MainEvent::InsetsChanged");
            WINDOW_RESIZED_PENDING.store(true, Ordering::Relaxed);
        }

        MainEvent::ContentRectChanged { .. } => {
            log::debug!("MainEvent::ContentRectChanged");
            WINDOW_RESIZED_PENDING.store(true, Ordering::Relaxed);
        }

        _ => {
            log::trace!("MainEvent: other variant");
        }
    }
}

// ── main loop helper (compat with existing code) ──────────────────────────────

/// Run one iteration of the event loop.
///
/// This is a compatibility wrapper for code that uses a manual poll loop.
/// Prefer `run_event_loop` for the standard event loop.
///
/// `timeout_ms` — how long to block waiting for events (milliseconds).
/// Pass `0` for non-blocking, `-1` to block indefinitely.
///
/// Returns `true` if the application should exit.
pub fn poll_events(timeout_ms: i32) -> bool {
    if let Some(platform) = PLATFORM.get() {
        if platform.should_quit() {
            return true;
        }
        platform.tick();
    }

    let app = match android_app() {
        Some(app) => app,
        None => return false,
    };

    let timeout = if timeout_ms < 0 {
        None
    } else {
        Some(Duration::from_millis(timeout_ms as u64))
    };

    app.poll_events(timeout, |event| match event {
        PollEvent::Main(main_event) => {
            handle_main_event(&app, main_event);
        }
        PollEvent::Wake => {}
        _ => {}
    });

    process_input_events(&app);

    // Drive the GPUI rendering pipeline (same as run_event_loop).
    if let Some(platform) = PLATFORM.get() {
        platform.flush_main_thread_tasks();
        if let Some(win) = platform.primary_window() {
            win.request_frame();
        }
    }

    false
}

// ── public init / run helpers ─────────────────────────────────────────────────

/// Install a panic hook that routes panic messages to logcat.
///
/// Call this early in `android_main` so that any subsequent panic is
/// visible via `adb logcat`.  Safe to call multiple times — each call
/// replaces the previous hook.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<dyn Any>".to_string()
        };
        if let Some(loc) = info.location() {
            log::error!(
                "PANIC at {}:{}:{}: {}",
                loc.file(),
                loc.line(),
                loc.column(),
                payload
            );
        } else {
            log::error!("PANIC: {}", payload);
        }
    }));
}

/// Store the current Activity's `AndroidApp` and create its
/// `AndroidPlatform`.
///
/// Must be called from `android_main` before `run_event_loop`.  A process can
/// host multiple successive Activity instances, so this replaces values from
/// the previous `android_main` thread instead of treating them as immutable
/// process singletons.
pub fn init_platform(app: &AndroidApp) -> Arc<AndroidPlatform> {
    INIT_WINDOW_DONE.store(false, Ordering::Relaxed);
    INIT_WINDOW_PENDING.store(false, Ordering::Relaxed);
    TERM_WINDOW_PENDING.store(false, Ordering::Relaxed);
    WINDOW_RESIZED_PENDING.store(false, Ordering::Relaxed);
    CONFIG_CHANGED_PENDING.store(false, Ordering::Relaxed);
    PAUSE_PENDING.store(false, Ordering::Relaxed);
    RESUME_PENDING.store(false, Ordering::Relaxed);
    EDITOR_TOUCH_CLAIMED.store(false, Ordering::Relaxed);
    IME_ANIMATION_END_PENDING.store(false, Ordering::Relaxed);
    NATIVE_EDITOR_IME_PREVIEW_VISIBLE.store(false, Ordering::Relaxed);
    record_native_editor_bounds(None);
    *PENDING_EDITOR_OP.lock() = None;

    let replaced_app = ANDROID_APP.replace(app.clone());
    log::info!(
        "init_platform: stored AndroidApp{}",
        if replaced_app.is_some() {
            " (replacing previous Activity)"
        } else {
            ""
        }
    );

    let platform = Arc::new(AndroidPlatform::new(false));
    log::info!("init_platform: AndroidPlatform created");

    PLATFORM.replace(Arc::clone(&platform));
    platform
}

// ── system chrome (status bar / navigation bar) ───────────────────────────────

/// Cached last-applied system chrome style.
///
/// `set_system_chrome` is called on every frame render.  The JNI calls it
/// makes (getWindow, setStatusBarColor, etc.) are View operations that can
/// contend with the Android UI thread and intermittently deadlock.
/// By caching the last applied style we skip the JNI calls entirely when
/// nothing changed — which is the common case.
#[allow(clippy::type_complexity)]
static LAST_CHROME_STYLE: std::sync::Mutex<
    Option<(Option<u32>, Option<u32>, crate::StatusBarContentStyle)>,
> = std::sync::Mutex::new(None);

/// Read the current system-bar and display-cutout insets from `WindowInsets`.
///
/// `content_rect` can stay at `(0, 0, width, height)` on Android 15 when the
/// app is rendered edge-to-edge. `WindowInsets` is the authoritative source
/// for the area that the status/navigation bars occupy.
fn window_system_bar_insets(env: &mut jni::Env<'_>) -> Result<(i32, i32, i32, i32), String> {
    let activity_obj = activity(env)?;
    let window = env
        .call_method(
            &activity_obj,
            jni::jni_str!("getWindow"),
            jni::jni_sig!("()Landroid/view/Window;"),
            &[],
        )
        .and_then(|v: jni::objects::JValueOwned| v.l())
        .map_err(|e| {
            env.exception_clear();
            e.to_string()
        })?;
    if window.is_null() {
        return Err("getWindow returned null".into());
    }

    let decor = env
        .call_method(
            &window,
            jni::jni_str!("getDecorView"),
            jni::jni_sig!("()Landroid/view/View;"),
            &[],
        )
        .and_then(|v: jni::objects::JValueOwned| v.l())
        .map_err(|e| {
            env.exception_clear();
            e.to_string()
        })?;
    if decor.is_null() {
        return Err("getDecorView returned null".into());
    }

    let insets = env
        .call_method(
            &decor,
            jni::jni_str!("getRootWindowInsets"),
            jni::jni_sig!("()Landroid/view/WindowInsets;"),
            &[],
        )
        .and_then(|v: jni::objects::JValueOwned| v.l())
        .map_err(|e| {
            env.exception_clear();
            e.to_string()
        })?;
    if insets.is_null() {
        return Err("getRootWindowInsets returned null".into());
    }

    // API 30+: systemBars() | displayCutout().
    let typed_insets = env
        .call_method(
            &insets,
            jni::jni_str!("getInsets"),
            jni::jni_sig!("(I)Landroid/graphics/Insets;"),
            &[JValue::Int(0x00000087)],
        )
        .and_then(|v: jni::objects::JValueOwned| v.l());

    if let Ok(typed_insets) = typed_insets {
        if !typed_insets.is_null() {
            let read = |env: &mut jni::Env<'_>, name| {
                env.get_field(&typed_insets, name, jni::jni_sig!("I"))
                    .and_then(|v| v.i())
                    .map_err(|e| {
                        env.exception_clear();
                        e.to_string()
                    })
            };
            let left = read(env, jni::jni_str!("left"))?;
            let top = read(env, jni::jni_str!("top"))?;
            let right = read(env, jni::jni_str!("right"))?;
            let bottom = read(env, jni::jni_str!("bottom"))?;
            return Ok((left, top, right, bottom));
        }
    } else {
        env.exception_clear();
    }

    // API 26–29 fallback.
    let read = |env: &mut jni::Env<'_>, name| {
        env.call_method(&insets, name, jni::jni_sig!("()I"), &[])
            .and_then(|v| v.i())
            .map_err(|e| {
                env.exception_clear();
                e.to_string()
            })
    };
    Ok((
        read(env, jni::jni_str!("getSystemWindowInsetLeft"))?,
        read(env, jni::jni_str!("getSystemWindowInsetTop"))?,
        read(env, jni::jni_str!("getSystemWindowInsetRight"))?,
        read(env, jni::jni_str!("getSystemWindowInsetBottom"))?,
    ))
}

/// Apply system chrome styling on Android.
///
/// Sets the status bar color, navigation bar color, and light/dark
/// status bar icons via JNI calls to `Window` and `WindowInsetsController`.
///
/// Must be called from the main (native) thread that has JNI access.
pub fn set_system_chrome(style: &crate::SystemChromeStyle) {
    let status_bar_color = style.status_bar_color;
    let navigation_bar_color = style.navigation_bar_color;
    let status_bar_style = style.status_bar_style;

    // Skip the (expensive, potentially deadlocking) JNI calls when nothing changed.
    {
        let key = (status_bar_color, navigation_bar_color, status_bar_style);
        let mut last = LAST_CHROME_STYLE.lock().unwrap();
        if *last == Some(key) {
            return;
        }
        *last = Some(key);
    }

    let result = with_env(|env| {
        let activity_obj = activity(env)?;

        // 1. Get the Window: activity.getWindow()
        let window = env
            .call_method(
                &activity_obj,
                jni::jni_str!("getWindow"),
                jni::jni_sig!("()Landroid/view/Window;"),
                &[],
            )
            .and_then(|v: jni::objects::JValueOwned| v.l())
            .map_err(|e| {
                env.exception_clear();
                e.to_string()
            })?;
        if window.is_null() {
            return Err("getWindow returned null".into());
        }

        // 2. Set status bar color if provided
        if let Some(color) = status_bar_color {
            let argb = (0xFF000000_u32 | color) as i32;
            let _ = env.call_method(
                &window,
                jni::jni_str!("setStatusBarColor"),
                jni::jni_sig!("(I)V"),
                &[JValue::Int(argb)],
            );
            env.exception_clear();
        }

        // 3. Set navigation bar color if provided
        if let Some(color) = navigation_bar_color {
            let argb = (0xFF000000_u32 | color) as i32;
            let _ = env.call_method(
                &window,
                jni::jni_str!("setNavigationBarColor"),
                jni::jni_sig!("(I)V"),
                &[JValue::Int(argb)],
            );
            env.exception_clear();
        }

        // 4. Set light/dark status bar icons via WindowInsetsController (API 30+)
        let insetsctl = env.call_method(
            &window,
            jni::jni_str!("getInsetsController"),
            jni::jni_sig!("()Landroid/view/WindowInsetsController;"),
            &[],
        );

        if let Ok(v) = insetsctl {
            if let Ok(ctl) = v.l() {
                if !ctl.is_null() {
                    let mask: i32 = 0x00000008;
                    let appearance: i32 = match status_bar_style {
                        crate::StatusBarContentStyle::Dark => 0x00000008,
                        crate::StatusBarContentStyle::Light => 0,
                    };
                    let _ = env.call_method(
                        &ctl,
                        jni::jni_str!("setSystemBarsAppearance"),
                        jni::jni_sig!("(II)V"),
                        &[JValue::Int(appearance), JValue::Int(mask)],
                    );
                    env.exception_clear();
                }
            }
        } else {
            env.exception_clear();

            if let Ok(decor) = env
                .call_method(
                    &window,
                    jni::jni_str!("getDecorView"),
                    jni::jni_sig!("()Landroid/view/View;"),
                    &[],
                )
                .and_then(|v: jni::objects::JValueOwned| v.l())
            {
                if !decor.is_null() {
                    if let Ok(current) = env
                        .call_method(
                            &decor,
                            jni::jni_str!("getSystemUiVisibility"),
                            jni::jni_sig!("()I"),
                            &[],
                        )
                        .and_then(|v: jni::objects::JValueOwned| v.i())
                    {
                        let new_flags = match status_bar_style {
                            crate::StatusBarContentStyle::Dark => current | 0x00002000,
                            crate::StatusBarContentStyle::Light => current & !0x00002000,
                        };
                        let _ = env.call_method(
                            &decor,
                            jni::jni_str!("setSystemUiVisibility"),
                            jni::jni_sig!("(I)V"),
                            &[JValue::Int(new_flags)],
                        );
                        env.exception_clear();
                    }
                }
            }
        }

        if let Ok((left, top, right, bottom)) = window_system_bar_insets(env) {
            if let Some(platform) = PLATFORM.get() {
                if let Some(window) = platform.primary_window() {
                    window.update_safe_area_insets(left, top, right, bottom);
                }
            }
        }

        Ok(())
    });

    if let Err(e) = result {
        log::warn!("set_system_chrome: {e}");
    }

    log::info!(
        "set_system_chrome: status_bar_color={:?}, nav_bar_color={:?}, style={:?}",
        style.status_bar_color,
        style.navigation_bar_color,
        style.status_bar_style
    );
}

// ── software keyboard (IME) control ───────────────────────────────────────────

/// Show the software keyboard on Android.
///
/// The activity owns a real editor view and dispatches the UI-thread work.
pub fn show_keyboard_android(keyboard_type: crate::KeyboardType) {
    let input_type = match keyboard_type {
        crate::KeyboardType::Default => 1,
        crate::KeyboardType::EmailAddress => 33,
        crate::KeyboardType::Phone => 3,
        crate::KeyboardType::NumberPad => 2,
        crate::KeyboardType::URL => 17,
        crate::KeyboardType::Decimal => 8194,
    };
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiShowKeyboard"),
            jni::jni_sig!("(I)V"),
            &[JValue::Int(input_type)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiShowKeyboard failed: {error}");
    }
}

/// Hide the software keyboard on Android.
///
pub fn hide_keyboard_android() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiHideKeyboard"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiHideKeyboard failed: {error}");
    }
}

/// End the native editor's focus/IME session while keeping it mounted as the
/// visible renderer, so taps over the input fall through to GPUI again.
pub fn blur_native_editor_android() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiBlurNativeEditor"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiBlurNativeEditor failed: {error}");
    }
}

/// Apply an external controlled-value change to the persistent EditText.
pub fn clear_native_editor_text_android() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiClearNativeEditorText"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiClearNativeEditorText failed: {error}");
    }
}

/// Reset the persistent EditText and its frame-batched mirror to a saved value.
pub fn reset_native_editor_text_android(text: &str, session: u64) {
    let text = text.to_owned();
    let result = with_env(|env| {
        let activity = activity(env)?;
        let value = env.new_string(text.as_str()).e()?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiResetNativeEditorText"),
            jni::jni_sig!("(Ljava/lang/String;J)V"),
            &[JValue::Object(&value), JValue::Long(session as i64)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiResetNativeEditorText failed: {error}");
    }
}

/// Keep the Java editor's queued mirror callbacks on the current session.
pub fn set_native_editor_session_android(session: u64) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiSetNativeEditorSession"),
            jni::jni_sig!("(J)V"),
            &[JValue::Long(session as i64)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiSetNativeEditorSession failed: {error}");
    }
}

/// Show (or refresh) the real native editor takeover over the focused input.
/// The host activity positions a genuine `EditText` on the input's bounds;
/// the system owns the keyboard, caret, selection handles, and edit menu.
pub fn show_native_editor_android(params: crate::NativeEditorParams) {
    // Parked during paint; applied after this frame's draw submits.
    *PENDING_EDITOR_OP.lock() = Some(PendingEditorOp::Show(params));
}

/// Apply an editor show immediately. Called from the post-draw flush.
fn show_native_editor_direct(params: crate::NativeEditorParams) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        let crate::NativeEditorParams {
            text,
            selection_utf16: (sel_start_utf16, sel_end_utf16),
            bounds: (bounds_x, bounds_y, bounds_w, bounds_h),
            clip_bounds,
            text_origin: (text_origin_x, text_origin_y),
            font_size,
            multiline,
            wrap_width,
            line_height,
            line_ranges_utf16,
            initial,
            force_text_sync,
            text_color,
            background_color,
            border_color,
            focus_border_color,
            border_width,
            placeholder,
            placeholder_color,
            corner_radius,
        } = params;
        let jtext = env.new_string(&text).map_err(|error| error.to_string())?;
        let jplaceholder = env
            .new_string(placeholder.as_deref().unwrap_or(""))
            .map_err(|error| error.to_string())?;
        let line_ranges = line_ranges_utf16
            .iter()
            .map(|(start, end)| format!("{start}:{end}"))
            .collect::<Vec<_>>()
            .join(",");
        let jline_ranges = env
            .new_string(&line_ranges)
            .map_err(|error| error.to_string())?;
        let (clip_x, clip_y, clip_width, clip_height) =
            clip_bounds.map_or((0.0, 0.0, 0.0, 0.0), |bounds| bounds);
        env.call_method(
            &activity,
            jni::jni_str!("gpuiShowNativeEditor"),
            jni::jni_sig!(
                "(Ljava/lang/String;IIFFFFFFFFFIIIIFLjava/lang/String;ILjava/lang/String;FFFFFZZZZ)V"
            ),
            &[
                JValue::Object(&jtext),
                JValue::Int(sel_start_utf16 as i32),
                JValue::Int(sel_end_utf16 as i32),
                JValue::Float(bounds_x),
                JValue::Float(bounds_y),
                JValue::Float(bounds_w),
                JValue::Float(bounds_h),
                JValue::Float(text_origin_x),
                JValue::Float(text_origin_y),
                JValue::Float(font_size),
                JValue::Float(wrap_width),
                JValue::Float(line_height),
                JValue::Int(text_color as i32),
                JValue::Int(background_color as i32),
                JValue::Int(border_color as i32),
                JValue::Int(focus_border_color as i32),
                JValue::Float(border_width),
                JValue::Object(&jplaceholder),
                JValue::Int(placeholder_color as i32),
                JValue::Object(&jline_ranges),
                JValue::Float(corner_radius),
                JValue::Float(clip_x),
                JValue::Float(clip_y),
                JValue::Float(clip_width),
                JValue::Float(clip_height),
                JValue::Bool(clip_bounds.is_some()),
                JValue::Bool(multiline),
                JValue::Bool(initial),
                JValue::Bool(force_text_sync),
            ],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiShowNativeEditor failed: {error}");
    }
}

/// Apply the editor operation parked during paint. Called right after each
/// frame's draw so the editor window never runs ahead of the submitted frame.
pub fn flush_pending_editor_op() {
    let pending = PENDING_EDITOR_OP.lock().take();
    match pending {
        Some(PendingEditorOp::Show(params)) => show_native_editor_direct(params),
        Some(PendingEditorOp::Hide) => hide_native_editor_direct(),
        None => {}
    }
}

/// Hide the native editor and restore the IME shadow view focus.
pub fn hide_native_editor_android() {
    // Parked during paint; applied after this frame's draw submits.
    *PENDING_EDITOR_OP.lock() = Some(PendingEditorOp::Hide);
}

/// Remove the editor window before the frame that removes its owning dialog.
/// Closing a dialog starts during input dispatch; waiting for the post-draw
/// flush leaves the editor floating above the settings page for one frame.
pub fn hide_native_editor_now() {
    PENDING_EDITOR_OP.lock().take();
    hide_native_editor_direct();
}

/// Translate the attached editor window by a logical y delta.
///
/// Dialog movement is detected while building the new GPUI frame. Applying
/// the translation immediately lets the platform window land on that frame;
/// parking it until after `draw` made the native editor trail the moving
/// dialog by one frame. The Cookie dialog additionally moves from Android's
/// IME inset callback via [`set_native_editor_ime_follow_android`].
pub fn translate_native_editor_android(delta_y: f32) {
    if delta_y != 0.0 {
        translate_native_editor_direct(delta_y);
    }
}

/// Move the Cookie editor from Android's IME inset callback instead of from
/// GPUI's render pass. The inset callback runs earlier in the same vsync, so
/// the separate editor window keeps up with the dialog's reflow.
pub fn set_native_editor_ime_follow_android(follow: bool) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiSetNativeEditorImeFollow"),
            jni::jni_sig!("(Z)V"),
            &[JValue::Bool(follow)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiSetNativeEditorImeFollow failed: {error}");
    }
}

/// Enable the Android-only Cookie handoff where GPUI owns the visual field
/// during IME animations and Java reveals the real editor after the final
/// submitted frame.
pub fn set_native_editor_ime_preview_android(preview: bool) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiSetNativeEditorImePreview"),
            jni::jni_sig!("(Z)V"),
            &[JValue::Bool(preview)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiSetNativeEditorImePreview failed: {error}");
    }
}

pub fn take_native_editor_ime_animation_end() -> bool {
    IME_ANIMATION_END_PENDING.swap(false, Ordering::AcqRel)
}

pub fn native_editor_ime_preview_visible() -> bool {
    NATIVE_EDITOR_IME_PREVIEW_VISIBLE.load(Ordering::Acquire)
}

pub fn set_native_editor_ime_preview_visible(visible: bool) {
    NATIVE_EDITOR_IME_PREVIEW_VISIBLE.store(visible, Ordering::Release);
    wake_ime_frame();
}

pub fn show_native_editor_after_ime_animation() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiShowNativeEditorAfterImeAnimation"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiShowNativeEditorAfterImeAnimation failed: {error}");
    }
}

/// Keep the editor window attached but transparent while a dialog warms up
/// its GPUI surface. The next call exposes the already-laid-out window.
pub fn set_native_editor_warmup_android(warmup: bool) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiSetNativeEditorWarmup"),
            jni::jni_sig!("(Z)V"),
            &[JValue::Bool(warmup)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiSetNativeEditorWarmup failed: {error}");
    }
}

fn translate_native_editor_direct(delta_y: f32) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiTranslateNativeEditor"),
            jni::jni_sig!("(F)V"),
            &[JValue::Float(delta_y)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiTranslateNativeEditor failed: {error}");
    }
}

/// Dismiss the Java resume cover after the first post-resume frame submits.
pub fn hide_resume_cover_android() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiHideResumeCover"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiHideResumeCover failed: {error}");
    }
}

/// Apply an editor hide immediately. Called from the post-draw flush.
fn hide_native_editor_direct() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiHideNativeEditor"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiHideNativeEditor failed: {error}");
    }
}

/// Read the live native editor text for a synchronous focus/commit path.
pub fn native_editor_text_android() -> Option<String> {
    with_env(|env| {
        if env.exception_check() {
            env.exception_clear();
        }
        let activity = activity(env)?;
        let _ = env.call_method(
            &activity,
            jni::jni_str!("gpuiFlushNativeEditorText"),
            jni::jni_sig!("()V"),
            &[],
        );
        if env.exception_check() {
            env.exception_clear();
        }
        let value = env
            .call_method(
                &activity,
                jni::jni_str!("gpuiGetNativeEditorText"),
                jni::jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })?;
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(get_string(env, &value)))
        }
    })
    .ok()
    .flatten()
    .map(|value| {
        crate::discard_pending_mirror_events();
        value
    })
}

/// Current display rotation as `Surface.ROTATION_*` (0/1/2/3).
pub fn display_rotation_android() -> Option<i32> {
    with_env(|env| {
        if env.exception_check() {
            env.exception_clear();
        }
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiDisplayRotation"),
            jni::jni_sig!("()I"),
            &[],
        )
        .and_then(|value| value.i())
        .map_err(|error| {
            env.exception_clear();
            error.to_string()
        })
    })
    .ok()
}

/// Deliver a key event to the system `EditText` that currently owns input.
pub fn dispatch_native_editor_key_android(action: i32, key_code: i32, meta_state: i32) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiDispatchNativeEditorKey"),
            jni::jni_sig!("(III)V"),
            &[
                JValue::Int(action),
                JValue::Int(key_code),
                JValue::Int(meta_state),
            ],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiDispatchNativeEditorKey failed: {error}");
    }
}

/// The mirror editor's text changed: sync into the focused GPUI input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeMirrorTextChanged(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    text: *mut std::ffi::c_void,
    sel_start: i32,
    sel_end: i32,
    session: i64,
) {
    let current = crate::native_editor_session();
    if (session as u64) < current {
        return;
    }
    if (session as u64) > current {
        crate::set_native_editor_session(session as u64);
    }
    let sel_start = sel_start.max(0) as usize;
    let sel_end = sel_end.max(0) as usize;
    enqueue_string_ime_event(text, move |text| crate::ImeEvent::MirrorText {
        text,
        sel_start_utf16: sel_start,
        sel_end_utf16: sel_end,
    });
}

/// The mirror editor's selection changed (handle drag): sync selection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeMirrorSelectionChanged(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    sel_start: i32,
    sel_end: i32,
) {
    crate::enqueue_ime_event(crate::ImeEvent::MirrorSelection {
        start_utf16: sel_start.max(0) as usize,
        end_utf16: sel_end.max(0) as usize,
    });
    wake_ime_frame();
}

/// The mirror editor scrolled: drive the GPUI input viewport with the same
/// offset so visible text and the system caret/selection stay together.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeMirrorScrollChanged(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    scroll_top: f32,
) {
    crate::enqueue_ime_event(crate::ImeEvent::MirrorScroll {
        scroll_top: scroll_top.max(0.0),
    });
    wake_ime_frame();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnNativeEditorHidden(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    crate::NATIVE_EDITOR_ACTIVE.store(false, Ordering::Release);
    record_native_editor_bounds(None);
    EDITOR_TOUCH_CLAIMED.store(false, Ordering::Release);
    crate::set_native_editor_line_ranges(None);
    crate::enqueue_ime_event(crate::ImeEvent::NativeEditorHidden);
    wake_ime_frame();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnNativeEditorFocusChanged(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    focused: jni::sys::jboolean,
) {
    if !focused {
        // The synchronous snapshot read on outside MouseDown handles the
        // latest editor contents. Drop any frame-batched echoes before the
        // focus-loss event can leave them queued for the next focused input.
        crate::discard_pending_mirror_events();
    }
    crate::enqueue_ime_event(crate::ImeEvent::NativeEditorFocusChanged(focused));
    wake_ime_frame();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnNativeEditorImeAnimationEnd(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    IME_ANIMATION_END_PENDING.store(true, Ordering::Release);
    wake_ime_frame();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnNativeEditorImePreviewVisible(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    visible: jni::sys::jboolean,
) {
    set_native_editor_ime_preview_visible(visible);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnNativeEditorWindowFocusChanged(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    owns_focus: jni::sys::jboolean,
) {
    crate::set_native_editor_owns_window_focus(owns_focus);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeMirrorLineRangesChanged(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    text: *mut std::ffi::c_void,
    ranges: *mut std::ffi::c_void,
) {
    let text_raw = text as jni::sys::jobject;
    let ranges_raw = ranges as jni::sys::jobject;
    let _ = with_env(|env| {
        let text = unsafe { JObject::from_raw(env, text_raw) };
        let ranges = unsafe { JObject::from_raw(env, ranges_raw) };
        let text = get_string(env, &text);
        let ranges = get_string(env, &ranges);
        let ranges_utf16 = ranges
            .split(',')
            .filter_map(|range| {
                let (start, end) = range.split_once(':')?;
                Some((start.parse().ok()?, end.parse().ok()?))
            })
            .collect();
        crate::set_native_editor_line_ranges(Some(crate::NativeEditorLineRanges {
            text,
            ranges_utf16,
        }));
        wake_ime_frame();
        Ok(())
    });
}

/// Queue a copy-selection action from the system text menu. Drained on the
/// next frame together with the IME events.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeCopySelection(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    crate::enqueue_ime_event(crate::ImeEvent::CopySelection);
}

/// Queue a cut-selection action from the system text menu. Drained on the
/// next frame together with the IME events.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeCutSelection(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    crate::enqueue_ime_event(crate::ImeEvent::CutSelection);
}

/// Store `text` in the Android system clipboard (text menu copy action).
pub fn set_clipboard_text_android(text: &str) {
    let result = with_env(|env| {
        let activity = activity(env)?;
        let jtext = env.new_string(text).map_err(|error| error.to_string())?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiSetClipboardText"),
            jni::jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::Object(&jtext)],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiSetClipboardText failed: {error}");
    }
}

pub fn notify_accessibility_changed() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiAccessibilityChanged"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiAccessibilityChanged failed: {error}");
    }
}

/// Reset the Android IME's bounded local context after GPUI switches the
/// focused input handler. The shadow never determines Rust-side edit ranges.
pub fn reset_ime_shadow() {
    let result = with_env(|env| {
        let activity = activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiResetInputShadow"),
            jni::jni_sig!("()V"),
            &[],
        )
        .e()?;
        Ok(())
    });
    if let Err(error) = result {
        log::warn!("gpuiResetInputShadow failed: {error}");
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

// ── JNI export for GpuiActivity splash screen ────────────────────────────────

/// Called from `GpuiActivity.nativeIsInitialized()` to check whether the
/// native library has finished initializing (first frame rendered).
///
/// The AndroidX SplashScreen API calls this via `setKeepOnScreenCondition`
/// to hold the splash visible until GPUI is ready.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeIsInitialized(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> u8 {
    if NATIVE_INITIALIZED.load(Ordering::Acquire) {
        1 // JNI_TRUE
    } else {
        0 // JNI_FALSE
    }
}

/// Called from `GpuiActivity` to query whether the first GPUI frame has been
/// submitted to the NativeActivity surface.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeIsFirstFramePresented(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> u8 {
    if NATIVE_FIRST_FRAME_PRESENTED.load(Ordering::Acquire) {
        1 // JNI_TRUE
    } else {
        0 // JNI_FALSE
    }
}

/// Reset the first-frame latch when a new Activity instance installs its
/// launch cover.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeResetFirstFramePresented(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    NATIVE_FIRST_FRAME_PRESENTED.store(false, Ordering::Release);
}

/// Called from `GpuiActivity` when the system back key / gesture fires.
///
/// The Java input overlay owns focus, so back keys do not reliably reach the
/// native input queue. Forwarding them here keeps the in-app page-back
/// animation working on every navigation mode.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnBackPressed(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    let Some(platform) = PLATFORM.get() else {
        return;
    };
    platform
        .foreground_executor()
        .dispatch_on_main_thread(move || {
            crate::dispatch_back_handler();
        });
}

/// Called from `GpuiActivity` when the resume cover starts fading out (after
/// the render-complete hold). Flushes the editor op parked during the covered
/// frames so the native input appears together with the revealed UI.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnResumeCoverFadeStart(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    RESUME_COVER_ACTIVE.store(false, Ordering::Release);
    flush_pending_editor_op();
}

/// Called from `GpuiActivity` when Android reports updated `WindowInsets`.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnInsetsChanged(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    ime_left: i32,
    ime_top: i32,
    ime_right: i32,
    ime_bottom: i32,
) {
    let Some(platform) = PLATFORM.get() else {
        return;
    };
    platform
        .foreground_executor()
        .dispatch_on_main_thread(move || {
            let Some(window) = platform.primary_window() else {
                return;
            };
            window.update_safe_area_insets(left, top, right, bottom);
            window.update_ime_insets(ime_left, ime_top, ime_right, ime_bottom);
            window.notify_insets_changed();
            window.request_frame();
        });
}

/// JNI bridge: receive a deeplink URL from `GpuiActivity.onNewIntent()`.
///
/// When the app is already running and a deeplink is opened (e.g. via
/// `adb shell am start -d gpui://video_player`), the Java side calls
/// this to notify the Rust deeplink handler.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread with a valid `url` jobject.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeOnDeepLink(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    url: *mut std::ffi::c_void,
) {
    // We already have a JVM attached on this thread (UI thread).
    // Use with_env to get a properly wrapped Env handle.
    let url_raw = url as jni::sys::jobject;
    let _ = with_env(|env| {
        let url_obj = unsafe { JObject::from_raw(env, url_raw) };
        let url_string = get_string(env, &url_obj);
        // Don't let the JObject be dropped (it's owned by the JNI call frame).
        if url_string.is_empty() {
            return Ok(());
        }
        log::info!("nativeOnDeepLink: {}", url_string);

        #[cfg(feature = "deeplink")]
        {
            crate::packages::deeplink::notify_deep_link(&url_string);
        }
        Ok(())
    });
}

fn enqueue_string_ime_event(
    value: *mut std::ffi::c_void,
    make_event: impl FnOnce(String) -> crate::ImeEvent,
) {
    let value_raw = value as jni::sys::jobject;
    let queued = with_env(|env| {
        let value = unsafe { JObject::from_raw(env, value_raw) };
        crate::enqueue_ime_event(make_event(get_string(env, &value)));
        Ok(())
    })
    .is_ok();
    if queued {
        wake_ime_frame();
    }
}

/// IME callbacks arrive from the Android UI thread, often while GPUI is idle.
/// Waking the dispatcher is not sufficient by itself: the queued IME event is
/// drained from the window frame callback, so request an actual frame.
pub(crate) fn wake_ime_frame() {
    if let Some(platform) = PLATFORM.get() {
        platform
            .foreground_executor()
            .dispatch_on_main_thread(move || {
                if let Some(window) = platform.primary_window() {
                    window.request_frame();
                }
            });
    }
}

/// Commit finalized text from Android's `InputConnection`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeCommitText(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    text: *mut std::ffi::c_void,
) {
    enqueue_string_ime_event(text, crate::ImeEvent::Commit);
}

/// Update the active marked/composing text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeSetComposingText(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    text: *mut std::ffi::c_void,
) {
    enqueue_string_ime_event(text, crate::ImeEvent::SetComposing);
}

/// Finish the active marked-text session.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeFinishComposingText(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    crate::enqueue_ime_event(crate::ImeEvent::FinishComposing);
}

/// Delete UTF-16 code units around the current selection.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeDeleteSurroundingText(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    before: i32,
    after: i32,
) {
    crate::enqueue_ime_event(crate::ImeEvent::DeleteSurrounding {
        before: before.max(0) as usize,
        after: after.max(0) as usize,
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeAccessibilitySnapshot(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    let snapshot = crate::accessibility::snapshot();
    let mut result = std::ptr::null_mut();
    let _ = with_env(|env| {
        result = env.new_string(snapshot).e()?.into_raw().cast();
        Ok(())
    });
    result
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiActivity_nativeAccessibilityAction(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    node_id: i64,
    action: i32,
) -> u8 {
    // Java forwards the AccessKit ID as a jlong bit pattern so the full u64
    // range survives JNI even when its signed representation is negative.
    u8::from(crate::accessibility::perform_action(node_id as u64, action))
}

/// JNI bridge: receive a media action from `GpuiMediaSession` system controls.
///
/// Actions: "play", "pause", "stop", "next", "previous"
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread with a valid `action` jobject.
#[cfg(feature = "media_session")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiMediaSession_nativeMediaAction(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    action: *mut std::ffi::c_void,
) {
    let action_raw = action as jni::sys::jobject;
    let _ = with_env(|env| {
        let action_obj = unsafe { JObject::from_raw(env, action_raw) };
        let action_str = get_string(env, &action_obj);

        let Some(media_action) = crate::packages::media_session::parse_media_action(&action_str)
        else {
            log::warn!("nativeMediaAction: unknown action '{action_str}'");
            return Ok(());
        };

        log::info!("nativeMediaAction: {:?}", media_action);
        crate::packages::media_session::notify_action(media_action);
        Ok(())
    });
}

/// JNI bridge: receive a seek request from `GpuiMediaSession` system controls.
///
/// # Safety
/// Must only be called from the JVM on a valid JNI thread.
#[cfg(feature = "media_session")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_dev_gpui_mobile_GpuiMediaSession_nativeMediaSeek(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    position_ms: i64,
) {
    log::info!("nativeMediaSeek: {}ms", position_ms);
    crate::packages::media_session::notify_seek(position_ms as u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_loop_blocks_without_frame_or_delayed_work() {
        assert_eq!(event_loop_poll_timeout(false, None), None);
        assert_eq!(
            event_loop_poll_timeout(false, Some(Duration::from_millis(25))),
            Some(Duration::from_millis(25))
        );
        assert_eq!(
            event_loop_poll_timeout(true, Some(Duration::from_secs(1))),
            Some(crate::event_stages::FRAME_POLL_INTERVAL)
        );
    }

    #[test]
    fn poll_events_returns_false_when_no_platform() {
        // PLATFORM is not set in a unit-test context, so poll_events should
        // be a safe no-op and return false (don't quit).
        let result = poll_events(0);
        let _ = result;
    }

    #[test]
    fn java_vm_returns_null_before_init() {
        // Before android_main is called, java_vm() should return null.
        let vm = java_vm();
        assert!(vm.is_null());
    }

    #[test]
    fn activity_as_ptr_returns_null_before_init() {
        let ptr = activity_as_ptr();
        assert!(ptr.is_null());
    }

    #[test]
    fn android_app_returns_none_before_init() {
        assert!(android_app().is_none());
    }

    #[test]
    fn platform_returns_none_before_init() {
        assert!(platform().is_none());
    }
}
