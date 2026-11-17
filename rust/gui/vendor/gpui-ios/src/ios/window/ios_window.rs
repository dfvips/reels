use super::super::IosDisplay;
use super::consts::GPUI_WINDOW_IVAR;
use super::foreign_parent::ForeignParentState;
use super::misc::ns_string_to_string;
use super::misc::query_scene_metrics;
use super::misc::view_safe_area_insets;
#[cfg(target_os = "ios")]
use super::register::input_diag_log;
use super::register::register_metal_view_class;
use super::register::register_text_input_view_class;
use super::register::register_view_controller_class;
#[cfg(target_os = "ios")]
use super::register::register_window_class;
use super::touch::ReentrancyGuard;
use super::types::{LongPressPending, PinchState, TouchStateMap};
use crate::momentum::{MomentumScroller, VelocityTracker};
use crate::native::{DynamicTypeCategory, IosSceneMetrics, SizeClass};
use crate::platform_view::NativePlatformViewHost;
use gpui::{
    AnyWindowHandle, Bounds, Capslock, DevicePixels, DispatchEventResult, GpuSpecs, Modifiers,
    Pixels, PlatformAtlas, PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow,
    Point, PromptButton, PromptLevel, RequestFrameOptions, Scene, Size, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowParams, WindowVisibility,
    px, size,
};
use gpui_wgpu::WgpuRenderer;
#[cfg(target_os = "ios")]
use objc::runtime::NO;
use objc::{
    class, msg_send,
    runtime::{BOOL, Object, YES},
    sel, sel_impl,
};
use parking_lot::Mutex;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, UiKitDisplayHandle, UiKitWindowHandle};

unsafe extern "C" {
    fn class_respondsToSelector(cls: *const Object, sel: objc::runtime::Sel) -> BOOL;
}

/// Foundation `NSRange` layout (64-bit `NSUInteger` fields).
#[repr(C)]
#[derive(Clone, Copy)]
struct NSRange {
    location: usize,
    length: usize,
}

/// Foundation `UIEdgeInsets` layout.
#[repr(C)]
#[derive(Clone, Copy)]
struct UIEdgeInsets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}

/// Build a `UIColor` from a `0xRRGGBBAA` value.
fn ui_color_from_rgba(rgba: u32) -> *mut Object {
    unsafe {
        let red = ((rgba >> 24) & 0xFF) as f64 / 255.0;
        let green = ((rgba >> 16) & 0xFF) as f64 / 255.0;
        let blue = ((rgba >> 8) & 0xFF) as f64 / 255.0;
        let alpha = (rgba & 0xFF) as f64 / 255.0;
        msg_send![class!(UIColor), colorWithRed: red green: green blue: blue alpha: alpha]
    }
}
use std::collections::HashSet;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    ffi::c_void,
    ptr::{self, NonNull},
    rc::Rc,
    sync::Arc,
};
use std::time::{Duration, Instant};

type dispatch_queue_t = *mut c_void;
type dispatch_time_t = u64;

const DISPATCH_TIME_NOW: dispatch_time_t = 0;

unsafe extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_after_f(
        when: dispatch_time_t,
        queue: dispatch_queue_t,
        context: *mut c_void,
        work: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn dispatch_time(when: dispatch_time_t, delta: i64) -> dispatch_time_t;
}

fn dispatch_get_main_queue() -> dispatch_queue_t {
    std::ptr::addr_of!(_dispatch_main_q) as *const _ as dispatch_queue_t
}

unsafe extern "C" fn main_task_trampoline(context: *mut c_void) {
    let task = Box::from_raw(context as *mut Box<dyn FnOnce()>);
    task();
}

fn dispatch_on_main_after(delay: Duration, task: Box<dyn FnOnce()>) {
    let when = unsafe {
        dispatch_time(
            DISPATCH_TIME_NOW,
            delay.as_nanos().min(i64::MAX as u128) as i64,
        )
    };
    unsafe {
        dispatch_after_f(
            when,
            dispatch_get_main_queue(),
            Box::into_raw(Box::new(task)) as *mut c_void,
            Some(main_task_trampoline),
        );
    }
}

fn keyboard_height_from_notification(notification: *mut Object) -> Option<f32> {
    if notification.is_null() {
        return None;
    }

    unsafe {
        let user_info: *mut Object = msg_send![notification, userInfo];
        if user_info.is_null() {
            return None;
        }
        let frame_key = super::super::ns_string_from_str("UIKeyboardFrameEndUserInfoKey");
        let frame_value: *mut Object = msg_send![user_info, objectForKey: frame_key];
        if frame_value.is_null() {
            return None;
        }
        let frame: core_graphics::geometry::CGRect = msg_send![frame_value, CGRectValue];
        Some(frame.size.height as f32)
    }
}

pub(crate) type RequestFrameCallback = RefCell<Option<Box<dyn FnMut(RequestFrameOptions)>>>;
pub(crate) type InputCallback =
    RefCell<Option<Box<dyn FnMut(PlatformInput) -> DispatchEventResult>>>;
pub(crate) type StatusCallback = RefCell<Option<Box<dyn FnMut(bool)>>>;
pub(crate) type ResizeCallback = RefCell<Option<Box<dyn FnMut(Size<Pixels>, f32)>>>;
pub(crate) type VoidCallback = RefCell<Option<Box<dyn FnMut()>>>;
pub(crate) type ShouldCloseCallback = RefCell<Option<Box<dyn FnMut() -> bool>>>;
pub(crate) type HitTestCallback = RefCell<Option<Box<dyn FnMut() -> Option<WindowControlArea>>>>;
pub(crate) type CloseCallback = RefCell<Option<Box<dyn FnOnce()>>>;

pub(crate) struct IosWindow {
    /// The UIWindow object
    pub(super) window: *mut Object,
    /// The UIViewController
    pub(super) view_controller: *mut Object,
    /// The Metal-backed UIView
    pub(super) view: *mut Object,
    /// The hidden text input view for keyboard input
    pub(super) text_input_view: *mut Object,
    /// Current bounds in pixels
    pub(super) bounds: Cell<Bounds<Pixels>>,
    /// Scale factor
    pub(super) scale_factor: Cell<f32>,
    /// Input handler for text input
    pub(super) input_handler: RefCell<Option<PlatformInputHandler>>,
    /// Callback for frame requests
    /// Note: pub(super) to allow ffi.rs to access this for the display link callback
    pub(in super::super) request_frame_callback: RequestFrameCallback,
    /// Coalesces touch/scroll forced renders until the next display-link tick.
    pub(in super::super) forced_frame_pending: Cell<bool>,
    /// Callback for input events
    pub(super) input_callback: InputCallback,
    /// Prevents nested UIKit/FFI input dispatch from borrowing the callback
    /// while it is already running. Nested events are delivered in order when
    /// the active callback returns.
    /// `pub(super)` so the foreign-parent constructor (sibling module) can
    /// build the window literal.
    pub(super) input_dispatching: Cell<bool>,
    pub(super) pending_input_events: RefCell<VecDeque<PlatformInput>>,
    /// Callback for active status changes
    pub(super) active_status_callback: StatusCallback,
    /// Callback for hover status changes (not really applicable on iOS)
    pub(super) hover_status_callback: StatusCallback,
    /// Callback for resize events
    pub(super) resize_callback: ResizeCallback,
    /// Callback for move events (not applicable on iOS)
    pub(super) moved_callback: VoidCallback,
    /// Callback for should close
    pub(super) should_close_callback: ShouldCloseCallback,
    /// Callback for hit test
    pub(super) hit_test_callback: HitTestCallback,
    /// Callback for close
    pub(super) close_callback: CloseCallback,
    /// Callback for appearance changes
    pub(super) appearance_changed_callback: VoidCallback,
    /// Current mouse position (from touch)
    pub(super) mouse_position: Cell<Point<Pixels>>,
    /// Current modifiers
    pub(super) modifiers: Cell<Modifiers>,
    /// Track if a touch is currently pressed
    pub(super) touch_pressed: Cell<bool>,
    /// Guards a scheduled synthetic mouse-away that clears hover state after
    /// a tap (touches have no native "mouse left the element" event).
    pub(super) hover_clear_scheduled: Cell<bool>,
    /// Time of the last software-keyboard show request; used to keep the
    /// hardware-keyboard focus helper from stealing first responder while a
    /// deferred `becomeFirstResponder` is still in flight.
    pub(super) last_keyboard_show: Cell<Option<Instant>>,
    /// Per-touch gesture state machine — distinguishes taps from scroll drags.
    /// Keyed by the UITouch pointer address.
    pub(super) touch_states: RefCell<TouchStateMap>,
    /// In-flight long-press recognizer state (see `LongPressPending`).
    pub(super) long_press: RefCell<Option<LongPressPending>>,
    /// Contacts claimed by an independent GPUI surface; these never enter
    /// the compatibility mouse, scroll, or pinch state machine.
    pub(super) direct_claimed_touches: RefCell<HashSet<usize>>,
    /// Whether the real `UITextView` editor is currently expanded over a
    /// focused GPUI input (native editor takeover).
    pub(super) native_editor_active: Cell<bool>,
    /// Last (background, text) colors applied to the native editor, as
    /// `0xRRGGBBAA`, to debounce per-frame refreshes.
    pub(super) native_editor_colors: Cell<(u32, u32)>,
    /// UIKit can be re-entered by application code invoked from an input
    /// callback. Retain and defer nested touches until their active state map
    /// is no longer borrowed.
    pub(super) touch_dispatching: Cell<bool>,
    pub(super) pending_touches: RefCell<VecDeque<*mut Object>>,
    /// Active two-finger pinch recognizer state.
    pub(super) pinch_state: RefCell<PinchState>,
    /// Velocity tracker — records recent touch samples during drag gestures
    /// so we can compute the release velocity when the finger lifts.
    pub(super) velocity_tracker: RefCell<VelocityTracker>,
    /// Momentum scroller — produces decelerating scroll deltas after a fling
    /// gesture, driven by the CADisplayLink frame callback.
    pub(super) momentum_scroller: RefCell<MomentumScroller>,
    pub(super) momentum_pumping: Cell<bool>,
    /// NotificationCenter observer tokens for keyboard show/hide callbacks.
    pub(super) keyboard_observers: RefCell<Vec<*mut Object>>,
    /// The wgpu renderer (Metal backend on iOS).
    /// Wrapped in a `Mutex<Option<…>>` so that `draw()` (called from the
    /// `request_frame` callback) can acquire a mutable reference without
    /// conflicting with the outer `&self` borrow.
    pub(super) renderer: Mutex<Option<WgpuRenderer>>,
    /// Cached atlas handle so `sprite_atlas()` does not need to lock the
    /// renderer mutex on every call once the renderer exists.
    pub(super) sprite_atlas: Mutex<Option<Arc<dyn PlatformAtlas>>>,
    /// Reusable UIKit accessibility elements keyed by accessibility node id.
    pub(super) accessibility_elements: RefCell<HashMap<String, *mut Object>>,
    /// Previous accessibility snapshot used to diff against the current one.
    /// Only mutated on the main thread.
    pub(super) prev_accessibility_snapshot:
        RefCell<Option<Arc<crate::accessibility::IosAccessibilitySnapshot>>>,
    /// Reusable index buffers for snapshot diffs; avoids per-refresh vectors.
    pub(super) accessibility_diff_scratch: RefCell<crate::accessibility::AccessibilityDiffScratch>,
    /// Native UIKit/SwiftUI views overlaid with GPUI-managed bounds.
    pub(super) platform_view_host: NativePlatformViewHost,
    /// Foreign-parent embedding state (frame throttle, resize debounce,
    /// host-driven focus/hover). Default-inert on the app path; driven by
    /// the foreign FFI entry points when this window has no `UIWindow`.
    pub(super) foreign: ForeignParentState,
}

unsafe impl Send for IosWindow {}

unsafe impl Sync for IosWindow {}

impl Drop for IosWindow {
    fn drop(&mut self) {
        self.unregister_keyboard_observers();
        unsafe {
            for touch in self.pending_touches.get_mut().drain(..) {
                if !touch.is_null() {
                    let _: () = msg_send![touch, release];
                }
            }
        }

        // Release any accessibility elements we retained for reuse.
        unsafe {
            for element in self.accessibility_elements.borrow_mut().drain() {
                if !element.1.is_null() {
                    let _: () = msg_send![element.1, release];
                }
            }
        }

        // Unregister from the global window list so lifecycle callbacks
        // don't dereference freed memory.
        super::super::ffi::unregister_window(self as *const Self);

        // Clear Objective-C ivars so post-destruction callbacks find null
        // instead of a dangling pointer.
        unsafe {
            if !self.view.is_null() {
                (*self.view).set_ivar(GPUI_WINDOW_IVAR, std::ptr::null::<c_void>());
            }
            if !self.text_input_view.is_null() {
                (*self.text_input_view).set_ivar(GPUI_WINDOW_IVAR, std::ptr::null::<c_void>());
            }
        }
    }
}

impl IosWindow {
    pub fn new(handle: AnyWindowHandle, _params: WindowParams) -> anyhow::Result<Self> {
        #[cfg(debug_assertions)]
        unsafe {
            let is_main: BOOL = msg_send![class!(NSThread), isMainThread];
            assert!(
                is_main == YES,
                "IosWindow must be created on the main thread"
            );
        }

        // Create the window on the main screen
        let screen = IosDisplay::main();
        let scale_factor = screen.scale();

        unsafe {
            // Create UIWindow
            let screen_obj: *mut Object = msg_send![class!(UIScreen), mainScreen];
            let screen_bounds_cg: core_graphics::geometry::CGRect = msg_send![screen_obj, bounds];
            #[cfg(target_os = "ios")]
            let window_class = register_window_class();
            #[cfg(not(target_os = "ios"))]
            let window_class = class!(UIWindow);
            let window: *mut Object = msg_send![window_class, alloc];
            let window: *mut Object = msg_send![window, initWithFrame: screen_bounds_cg];
            #[cfg(target_os = "ios")]
            input_diag_log(|| "window using legacy initWithFrame".to_owned());
            #[cfg(target_os = "ios")]
            input_diag_log(|| {
                format!("window created temp_dir={}", std::env::temp_dir().display())
            });

            // Create our custom UIViewController subclass that supports
            // dynamic `preferredStatusBarStyle` overrides.
            let vc_class = register_view_controller_class();
            let view_controller: *mut Object = msg_send![vc_class, alloc];
            let view_controller: *mut Object = msg_send![view_controller, init];

            // Create our custom Metal view using the registered class
            let metal_view_class = register_metal_view_class();
            let view: *mut Object = msg_send![metal_view_class, alloc];
            let view: *mut Object = msg_send![view, initWithFrame: screen_bounds_cg];

            // Configure the Metal layer — wgpu will use it for rendering but
            // we still need to set contentsScale so the drawable size is correct.
            let layer: *mut Object = msg_send![view, layer];
            let scale: core_graphics::base::CGFloat = msg_send![screen_obj, scale];
            let _: () = msg_send![layer, setContentsScale: scale];

            // Auto-resize the Metal view when the parent view changes size
            // (e.g. rotation). UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight
            let _: () = msg_send![view, setAutoresizingMask: 18_usize]; // 0x02 | 0x10

            // Enable user interaction on the Metal view for touch handling
            let _: () = msg_send![view, setUserInteractionEnabled: YES];
            let _: () = msg_send![view, setMultipleTouchEnabled: YES];

            #[cfg(target_os = "ios")]
            {
                // iPad pointer devices and the iOS Simulator deliver trackpad
                // and mouse-wheel scrolls through a pan recognizer configured
                // for indirect scroll types, not through touchesMoved.
                let recognizer: *mut Object = msg_send![class!(UIPanGestureRecognizer), alloc];
                let recognizer: *mut Object =
                    msg_send![recognizer, initWithTarget: view action: sel!(handleIndirectScroll:)];
                let _: () = msg_send![recognizer, setAllowedScrollTypesMask: 3_isize];
                let _: () = msg_send![recognizer, setDelegate: view];
                let _: () = msg_send![recognizer, setRequiresExclusiveTouchType: NO];
                let _: () = msg_send![recognizer, setCancelsTouchesInView: NO];
                let _: () = msg_send![recognizer, setDelaysTouchesBegan: NO];
                let _: () = msg_send![recognizer, setDelaysTouchesEnded: NO];
                let _: () = msg_send![view, addGestureRecognizer: recognizer];
                input_diag_log(|| "installed indirect scroll pan recognizer".to_owned());
            }

            // Set the view as the view controller's view
            let _: () = msg_send![view_controller, setView: view];

            // Set the root view controller
            let _: () = msg_send![window, setRootViewController: view_controller];

            // Make the window visible
            let _: () = msg_send![window, makeKeyAndVisible];

            // Create a hidden text input view for keyboard handling.
            // Uses our custom GPUITextInputView which implements UIKeyInput
            // so iOS actually routes keyboard text to us.
            let text_input_class = register_text_input_view_class();
            let text_input_view: *mut Object = msg_send![text_input_class, alloc];
            let text_input_frame = core_graphics::geometry::CGRect {
                origin: core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 },
                size: core_graphics::geometry::CGSize {
                    width: 1.0,
                    height: 1.0,
                },
            };
            let text_input_view: *mut Object =
                msg_send![text_input_view, initWithFrame: text_input_frame];
            let _: () = msg_send![text_input_view, setAlpha: 0.01_f64];
            let _: () = msg_send![text_input_view, setUserInteractionEnabled: YES];
            let _: () = msg_send![view, addSubview: text_input_view];

            // UIKit may resolve the actual scene size only after the window is
            // visible, especially on iPad in Split View or Stage Manager. Force
            // a layout pass now so the first GPUI render uses the current view
            // bounds instead of the full screen bounds.
            let _: () = msg_send![window, layoutIfNeeded];
            let _: () = msg_send![view, layoutIfNeeded];

            let initial_metrics =
                query_scene_metrics(view, scale_factor).unwrap_or_else(|| IosSceneMetrics {
                    width: screen_bounds_cg.size.width as f32,
                    height: screen_bounds_cg.size.height as f32,
                    scale_factor,
                    horizontal_size_class: SizeClass::Unspecified,
                    vertical_size_class: SizeClass::Unspecified,
                    dynamic_type: DynamicTypeCategory::Medium,
                    safe_area: view_safe_area_insets(view),
                    keyboard_height: crate::keyboard_height(),
                });
            let initial_bounds = Bounds {
                origin: Default::default(),
                size: size(px(initial_metrics.width), px(initial_metrics.height)),
            };
            let initial_scale = initial_metrics.scale_factor as core_graphics::base::CGFloat;
            let _: () = msg_send![layer, setContentsScale: initial_scale];

            // --- Initialise the wgpu renderer (Metal backend) ---------------
            let pixel_w = (initial_metrics.width * initial_metrics.scale_factor)
                .round()
                .max(1.0) as i32;
            let pixel_h = (initial_metrics.height * initial_metrics.scale_factor)
                .round()
                .max(1.0) as i32;

            let ios_window = Self {
                window,
                view_controller,
                view,
                text_input_view,
                bounds: Cell::new(initial_bounds),
                scale_factor: Cell::new(initial_metrics.scale_factor),
                input_handler: RefCell::new(None),
                request_frame_callback: RefCell::new(None),
                forced_frame_pending: Cell::new(false),
                input_callback: RefCell::new(None),
                input_dispatching: Cell::new(false),
                pending_input_events: RefCell::new(VecDeque::new()),
                active_status_callback: RefCell::new(None),
                hover_status_callback: RefCell::new(None),
                resize_callback: RefCell::new(None),
                moved_callback: RefCell::new(None),
                should_close_callback: RefCell::new(None),
                hit_test_callback: RefCell::new(None),
                close_callback: RefCell::new(None),
                appearance_changed_callback: RefCell::new(None),
                mouse_position: Cell::new(Point::default()),
                modifiers: Cell::new(Modifiers::default()),
                touch_pressed: Cell::new(false),
                hover_clear_scheduled: Cell::new(false),
                last_keyboard_show: Cell::new(None),
                touch_states: RefCell::new(TouchStateMap::new()),
                long_press: RefCell::new(None),
                direct_claimed_touches: RefCell::new(HashSet::new()),
                native_editor_active: Cell::new(false),
                native_editor_colors: Cell::new((0, 0)),
                touch_dispatching: Cell::new(false),
                pending_touches: RefCell::new(VecDeque::new()),
                pinch_state: RefCell::new(PinchState::default()),
                velocity_tracker: RefCell::new(VelocityTracker::new()),
                momentum_scroller: RefCell::new(MomentumScroller::new()),
                momentum_pumping: Cell::new(false),
                keyboard_observers: RefCell::new(Vec::new()),
                renderer: Mutex::new(None),
                sprite_atlas: Mutex::new(None),
                accessibility_elements: RefCell::new(HashMap::new()),
                prev_accessibility_snapshot: RefCell::new(None),
                accessibility_diff_scratch: RefCell::new(Default::default()),
                platform_view_host: NativePlatformViewHost::new(view as *mut c_void),
                foreign: ForeignParentState::default(),
            };

            // Keep app-level hardware keyboard shortcuts alive when no text
            // input is active.
            ios_window.focus_hardware_keyboard_view();

            // Create the Metal-backed wgpu renderer (see `renderer.rs`).
            super::renderer::init_metal_renderer(
                &ios_window,
                handle,
                size(DevicePixels(pixel_w), DevicePixels(pixel_h)),
            )?;

            Ok(ios_window)
        }
    }

    /// Get the raw pointer to the UIViewController.
    #[allow(
        dead_code,
        reason = "kept for Swift/Objective-C integration points that are not used by the Rust demo"
    )]
    pub fn view_controller_ptr(&self) -> *mut Object {
        self.view_controller
    }

    /// Get the raw pointer to the GPUIMetalView.
    #[allow(
        dead_code,
        reason = "kept for Swift/Objective-C integration points that are not used by the Rust demo"
    )]
    pub fn metal_view_ptr(&self) -> *mut Object {
        self.view
    }

    /// Register this window with the FFI layer after it's been stored.
    /// This must be called after the window is placed at a stable address
    /// (e.g., in a Box or Arc).
    pub(crate) fn register_with_ffi(&self) {
        super::super::ffi::register_window(self as *const Self);
        self.set_view_ivars();

        // Listen for keyboard show/hide so we can expose the keyboard height.
        self.register_keyboard_observers();
    }

    /// Set the window pointer on the Metal view so touch events can find us,
    /// and on the text input view so keyboard input can find us.
    pub(super) fn set_view_ivars(&self) {
        unsafe {
            let window_ptr = self as *const Self as *mut std::ffi::c_void;
            (*self.view).set_ivar(GPUI_WINDOW_IVAR, window_ptr);
            (*self.text_input_view).set_ivar(GPUI_WINDOW_IVAR, window_ptr);
            log::info!(
                "GPUI iOS: Set window pointer {:p} on view {:p} and text input {:p}",
                window_ptr,
                self.view,
                self.text_input_view
            );
        }
    }

    /// Register for keyboard show/hide notifications so we can track the
    /// keyboard height and allow the UI to shift content above the keyboard.
    pub(crate) fn register_keyboard_observers(&self) {
        if !self.keyboard_observers.borrow().is_empty() {
            return;
        }

        unsafe {
            let notification_center: *mut Object =
                msg_send![class!(NSNotificationCenter), defaultCenter];

            let show_name = super::super::ns_string_from_str("UIKeyboardWillShowNotification");
            let hide_name = super::super::ns_string_from_str("UIKeyboardWillHideNotification");
            let frame_name =
                super::super::ns_string_from_str("UIKeyboardWillChangeFrameNotification");
            let input_mode_name = super::super::ns_string_from_str(
                "UITextInputCurrentInputModeDidChangeNotification",
            );

            // Block that fires when the keyboard appears. It extracts the
            // end-frame height and stores it in the global atomic.
            // The closure takes `*mut c_void` because block2 only encodes
            // C-ABI types; we cast back to `*mut Object` inside.
            let show_block = block2::RcBlock::new(move |notification: *mut c_void| {
                if let Some(height) = keyboard_height_from_notification(notification as *mut Object)
                {
                    log::info!("GPUI iOS: Keyboard will show, height={}", height);
                    crate::set_keyboard_height(height);
                }
            });

            let hide_block = block2::RcBlock::new(move |_notification: *mut c_void| {
                log::info!("GPUI iOS: Keyboard will hide");
                crate::set_keyboard_height(0.0);
            });

            let frame_block = block2::RcBlock::new(move |notification: *mut c_void| {
                if let Some(height) = keyboard_height_from_notification(notification as *mut Object)
                {
                    log::info!("GPUI iOS: Keyboard frame changed, height={}", height);
                    crate::set_keyboard_height(height);
                }
            });

            let input_mode_block = block2::RcBlock::new(move |_notification: *mut c_void| {
                log::info!("GPUI iOS: Keyboard input mode changed");
                crate::dispatch_keyboard_layout_change();
            });

            let show_observer: *mut Object = msg_send![notification_center,
                addObserverForName: show_name
                object: std::ptr::null::<Object>()
                queue: std::ptr::null::<Object>()
                usingBlock: &*show_block
            ];
            let hide_observer: *mut Object = msg_send![notification_center,
                addObserverForName: hide_name
                object: std::ptr::null::<Object>()
                queue: std::ptr::null::<Object>()
                usingBlock: &*hide_block
            ];
            let frame_observer: *mut Object = msg_send![notification_center,
                addObserverForName: frame_name
                object: std::ptr::null::<Object>()
                queue: std::ptr::null::<Object>()
                usingBlock: &*frame_block
            ];
            let input_mode_observer: *mut Object = msg_send![notification_center,
                addObserverForName: input_mode_name
                object: std::ptr::null::<Object>()
                queue: std::ptr::null::<Object>()
                usingBlock: &*input_mode_block
            ];

            let mut observers = self.keyboard_observers.borrow_mut();
            if !show_observer.is_null() {
                observers.push(show_observer);
            }
            if !hide_observer.is_null() {
                observers.push(hide_observer);
            }
            if !frame_observer.is_null() {
                observers.push(frame_observer);
            }
            if !input_mode_observer.is_null() {
                observers.push(input_mode_observer);
            }
        }
    }

    pub(super) fn unregister_keyboard_observers(&self) {
        let mut observers = self.keyboard_observers.borrow_mut();
        if observers.is_empty() {
            return;
        }

        unsafe {
            let notification_center: *mut Object =
                msg_send![class!(NSNotificationCenter), defaultCenter];
            for observer in observers.drain(..) {
                if !observer.is_null() {
                    let _: () = msg_send![notification_center, removeObserver: observer];
                }
            }
        }
    }

    pub(super) fn dispatch_input(&self, input: PlatformInput) -> DispatchEventResult {
        if self.input_dispatching.replace(true) {
            self.pending_input_events.borrow_mut().push_back(input);
            return DispatchEventResult::default();
        }
        let _dispatch_guard = ReentrancyGuard(&self.input_dispatching);

        let mut first_result = None;
        let mut next_input = Some(input);
        while let Some(input) = next_input {
            let mut callback = self.input_callback.borrow_mut().take();
            let result = callback
                .as_mut()
                .map_or_else(DispatchEventResult::default, |callback| callback(input));

            if self.input_callback.borrow().is_none() {
                *self.input_callback.borrow_mut() = callback;
            }

            first_result.get_or_insert(result);
            next_input = self.pending_input_events.borrow_mut().pop_front();
        }

        first_result.unwrap_or_default()
    }

    pub(super) fn request_forced_frame(&self) {
        if self.forced_frame_pending.replace(true) {
            return;
        }
        let mut callback = self.request_frame_callback.borrow_mut().take();
        if let Some(callback) = callback.as_mut() {
            callback(RequestFrameOptions {
                force_render: true,
                ..Default::default()
            });
        } else {
            self.forced_frame_pending.set(false);
        }
        if callback.is_some() && self.request_frame_callback.borrow().is_none() {
            *self.request_frame_callback.borrow_mut() = callback;
        }
    }

    /// Query the safe area insets from the UIView.
    ///
    /// Returns `(top, left, bottom, right)` in logical points (matching
    /// `UIEdgeInsets` field order — see implementation below). These
    /// represent the areas occupied by system UI (status bar, home
    /// indicator, camera notch) that content should avoid.
    pub fn safe_area_insets(&self) -> (f32, f32, f32, f32) {
        let insets = unsafe { view_safe_area_insets(self.view) };
        (insets.top, insets.left, insets.bottom, insets.right)
    }

    /// Query current scene metrics from the actual UIKit view.
    pub fn scene_metrics(&self) -> Option<IosSceneMetrics> {
        unsafe { query_scene_metrics(self.view, self.scale_factor.get()) }
    }

    pub fn attach_to_parent_view(&self, parent: *mut c_void) {
        if parent.is_null() || self.view.is_null() {
            return;
        }
        self.platform_view_host.set_parent_view(parent);
        unsafe {
            // SAFETY: `parent` is supplied by Swift as a live UIView pointer for
            // the duration of this call, and `self.view` is the retained GPUI
            // Metal UIView owned by this window on the main thread.
            let parent = parent as *mut Object;
            let _: () = msg_send![parent, addSubview: self.view];
            let bounds: core_graphics::geometry::CGRect = msg_send![parent, bounds];
            let _: () = msg_send![self.view, setFrame: bounds];
            let _: () = msg_send![self.view, setAutoresizingMask: 18_usize];
        }
        crate::instrumentation::emit_signpost(
            crate::instrumentation::IosSignpostCategory::PlatformView,
            "attach_gpui_host_view",
        );
        self.handle_layout_change();
        self.focus_hardware_keyboard_view();
    }

    pub fn detach_from_parent_view(&self) {
        if self.view.is_null() {
            return;
        }
        unsafe {
            // SAFETY: `self.view` is a live UIView owned by this window. UIKit
            // permits `removeFromSuperview` even when there is no superview.
            let _: () = msg_send![self.view, removeFromSuperview];
        }
        crate::instrumentation::emit_signpost(
            crate::instrumentation::IosSignpostCategory::PlatformView,
            "detach_gpui_host_view",
        );
    }

    /// Show the software keyboard with the specified keyboard type.
    ///
    /// The actual `becomeFirstResponder` call is deferred to the next run-loop
    /// iteration via `performSelector:withObject:afterDelay:` to avoid re-entering
    /// GPUI's event dispatch while an entity lease is active (UIKit's keyboard
    /// presentation can synchronously trigger layout callbacks).
    pub fn show_keyboard_with_type(&self, keyboard_type: crate::KeyboardType) {
        log::info!("GPUI iOS: Showing keyboard (type={:?})", keyboard_type);
        self.last_keyboard_show.set(Some(Instant::now()));
        unsafe {
            use crate::KeyboardType;
            let kb_type: isize = match keyboard_type {
                KeyboardType::Default => 0,      // UIKeyboardTypeDefault
                KeyboardType::EmailAddress => 7, // UIKeyboardTypeEmailAddress
                KeyboardType::Phone => 5,        // UIKeyboardTypePhonePad
                KeyboardType::NumberPad => 4,    // UIKeyboardTypeNumberPad
                KeyboardType::URL => 3,          // UIKeyboardTypeURL
                KeyboardType::Decimal => 8,      // UIKeyboardTypeDecimalPad
            };
            log::info!(
                "GPUI iOS: text_input_view={:p}, setKeyboardType: {}",
                self.text_input_view,
                kb_type
            );
            if self.text_input_view.is_null() {
                log::error!("GPUI iOS: text_input_view is NULL!");
                return;
            }
            let _: () = msg_send![self.text_input_view, setKeyboardType: kb_type];
            log::info!("GPUI iOS: setAutocorrectionType");
            let _: () = msg_send![self.text_input_view, setAutocorrectionType: 1_isize];
            log::info!("GPUI iOS: setAutocapitalizationType");
            let _: () = msg_send![self.text_input_view, setAutocapitalizationType: 0_isize];
            log::info!("GPUI iOS: scheduling becomeFirstResponder");

            // Defer becomeFirstResponder to the next run-loop iteration.
            let _: () = msg_send![self.text_input_view,
                performSelector: sel!(becomeFirstResponder)
                withObject: ptr::null::<Object>()
                afterDelay: 0.0_f64
            ];
            log::info!("GPUI iOS: show_keyboard_with_type done");
        }
        self.schedule_keyboard_retry();
    }

    /// Some UIKit timing paths (foreground transitions, display-link pressure)
    /// swallow the first deferred `becomeFirstResponder`.  Re-check shortly
    /// afterwards and issue it directly if the text input never became first
    /// responder.
    fn schedule_keyboard_retry(&self) {
        let window_ptr: *mut IosWindow = self as *const IosWindow as *mut IosWindow;
        dispatch_on_main_after(
            Duration::from_millis(150),
            Box::new(move || unsafe {
                let window = &*window_ptr;
                if window.text_input_view.is_null() {
                    return;
                }
                let is_first_responder: BOOL =
                    msg_send![window.text_input_view, isFirstResponder];
                if is_first_responder != YES {
                    log::info!("GPUI iOS: keyboard retry — scheduling becomeFirstResponder");
                    let _: () = msg_send![window.text_input_view, becomeFirstResponder];
                }
            }),
        );
    }

    /// Hide the software keyboard.
    ///
    /// Deferred to the next run-loop iteration (like `show_keyboard_with_type`)
    /// to avoid re-entering GPUI event dispatch.
    pub fn hide_keyboard(&self) {
        log::info!("GPUI iOS: Hiding keyboard");
        // Collapse the native editor takeover as part of dismissing the
        // keyboard so the 1x1 hidden input view is restored.
        self.hide_native_editor();
        unsafe {
            let _: () = msg_send![self.text_input_view,
                performSelector: sel!(resignFirstResponder)
                withObject: ptr::null::<Object>()
                afterDelay: 0.0_f64
            ];
        }
        self.focus_hardware_keyboard_view();
    }

    /// Pull the native editor's committed text and selection into the GPUI
    /// input handler. Returns `false` when the handler borrow is busy
    /// (re-entrant dispatch); callers should retry on the next pass.
    pub(super) fn sync_editor_to_handler(&self) -> bool {
        let editor = self.text_input_view;
        if editor.is_null() {
            return true;
        }
        unsafe {
            let editor_text: *mut Object = msg_send![editor, text];
            let editor_text_str = ns_string_to_string(editor_text).unwrap_or_default();
            let selection: NSRange = msg_send![editor, selectedRange];
            let Ok(mut handler) = self.input_handler.try_borrow_mut() else {
                return false;
            };
            let Some(handler) = handler.as_mut() else {
                return true;
            };
            let current_len = handler.text_length_utf16().unwrap_or(0);
            let current_text = handler
                .text_for_range(0..current_len, &mut None)
                .unwrap_or_default();
            if current_text != editor_text_str {
                handler.set_selected_text_range(0..usize::MAX);
                handler.replace_text_in_range(None, &editor_text_str);
            }
            handler.set_selected_text_range(
                selection.location..selection.location + selection.length,
            );
        }
        true
    }

    pub fn native_editor_text(&self) -> Option<String> {
        if !self.native_editor_active.get() || self.text_input_view.is_null() {
            return None;
        }
        unsafe {
            let marked: *mut Object = msg_send![self.text_input_view, markedTextRange];
            if !marked.is_null() {
                let _: () = msg_send![self.text_input_view, unmarkText];
            }
            let editor_text: *mut Object = msg_send![self.text_input_view, text];
            ns_string_to_string(editor_text)
        }
    }

    /// Show (or refresh) the real native editor takeover for the focused
    /// GPUI input. The existing `GPUITextInputView` (a real `UITextView`
    /// subclass) is expanded over the input's bounds so the system owns the
    /// keyboard, caret, selection handles, and its own edit menu.
    ///
    /// The initial call seeds the editor from GPUI's state. Refresh calls
    /// (per frame) treat the editor as the source of truth and pull any
    /// native-only mutations — paste, cut, handle drags, caret moves — back
    /// into the GPUI input handler.
    pub fn show_native_editor(&self, params: &crate::NativeEditorParams) {
        if self.view.is_null() || self.text_input_view.is_null() {
            return;
        }
        let editor = self.text_input_view;
        // A focused GPUI input with a collapsed editor means the takeover
        // was dismissed earlier (e.g. hide_keyboard); re-seed it fully.
        let initial = params.initial || !self.native_editor_active.get();
        unsafe {
            // Inset by 1pt so the GPUI-drawn focus border stays visible
            // around the takeover editor.
            let frame = core_graphics::geometry::CGRect {
                origin: core_graphics::geometry::CGPoint {
                    x: params.bounds.0 as f64 + 1.0,
                    y: params.bounds.1 as f64 + 1.0,
                },
                size: core_graphics::geometry::CGSize {
                    width: (params.bounds.2 - 2.0).max(1.0) as f64,
                    height: (params.bounds.3 - 2.0).max(1.0) as f64,
                },
            };
            let _: () = msg_send![editor, setFrame: frame];
            let _: () = msg_send![editor, setAlpha: 1.0_f64];
            let _: () = msg_send![editor, setHidden: NO];
            let layer: *mut Object = msg_send![editor, layer];
            if !layer.is_null() {
                let _: () = msg_send![layer, setCornerRadius: params.corner_radius as f64];
            }
            let colors = (params.background_color, params.text_color);
            if colors != self.native_editor_colors.get() {
                let _: () = msg_send![
                    editor,
                    setBackgroundColor: ui_color_from_rgba(params.background_color)
                ];
                let _: () =
                    msg_send![editor, setTextColor: ui_color_from_rgba(params.text_color)];
                // The tint color drives the caret and the system drag handles.
                let _: () =
                    msg_send![editor, setTintColor: ui_color_from_rgba(params.text_color)];
                self.native_editor_colors.set(colors);
            }
            // Match the editor's font to the GPUI-drawn text so the takeover
            // is visually seamless with the placeholder line height.
            let ui_font: *mut Object =
                msg_send![class!(UIFont), systemFontOfSize: params.font_size as f64];
            let _: () = msg_send![editor, setFont: ui_font];
            // Align the editor's text origin with the GPUI-drawn text so the
            // takeover is visually seamless.
            let text_container: *mut Object = msg_send![editor, textContainer];
            if !text_container.is_null() {
                let _: () = msg_send![text_container, setLineFragmentPadding: 0.0_f64];
            }
            let left_inset = (params.text_origin.0 - params.bounds.0 - 1.0).max(0.0);
            let top_inset = (params.text_origin.1 - params.bounds.1 - 1.0).max(0.0);
            // Wrap at exactly the width GPUI measured for its text area so
            // focused (native) and blurred (GPUI-drawn) lines break
            // identically. Mirroring the left inset is the legacy fallback.
            let right_inset = if params.wrap_width > 0.0 {
                (frame.size.width as f32 - left_inset - params.wrap_width).max(0.0)
            } else {
                (params.text_origin.0 - params.bounds.0 - 1.0).max(0.0)
            };
            let _: () = msg_send![editor,
                setTextContainerInset: UIEdgeInsets {
                    top: top_inset as f64,
                    left: left_inset as f64,
                    bottom: 0.0,
                    right: right_inset as f64,
                }
            ];
            self.native_editor_active.set(true);

            let is_first_responder: BOOL = msg_send![editor, isFirstResponder];
            if initial {
                let ns_text = crate::ios::ns_string_from_str(&params.text);
                let _: () = msg_send![editor, setText: ns_text];
                let text_len = params.text.encode_utf16().count();
                if params.line_height > 0.0 {
                    let paragraph_style: *mut Object =
                        msg_send![class!(NSMutableParagraphStyle), new];
                    let _: () = msg_send![
                        paragraph_style,
                        setMinimumLineHeight: params.line_height as f64
                    ];
                    let _: () = msg_send![
                        paragraph_style,
                        setMaximumLineHeight: params.line_height as f64
                    ];
                    let _: () = msg_send![paragraph_style, setLineSpacing: 0.0_f64];
                    let attributed_text: *mut Object = msg_send![editor, attributedText];
                    let mutable_text: *mut Object = msg_send![attributed_text, mutableCopy];
                    let paragraph_key =
                        crate::ios::ns_string_from_str("NSParagraphStyle");
                    let font_key = crate::ios::ns_string_from_str("NSFont");
                    let color_key = crate::ios::ns_string_from_str("NSColor");
                    let color = ui_color_from_rgba(params.text_color);
                    let _: () = msg_send![mutable_text,
                        addAttribute: paragraph_key
                        value: paragraph_style
                        range: NSRange {
                            location: 0,
                            length: text_len,
                        }
                    ];
                    let _: () = msg_send![mutable_text,
                        addAttribute: font_key
                        value: ui_font
                        range: NSRange {
                            location: 0,
                            length: text_len,
                        }
                    ];
                    let _: () = msg_send![mutable_text,
                        addAttribute: color_key
                        value: color
                        range: NSRange {
                            location: 0,
                            length: text_len,
                        }
                    ];
                    let _: () = msg_send![editor, setAttributedText: mutable_text];

                    let typing_attributes: *mut Object =
                        msg_send![class!(NSMutableDictionary), dictionary];
                    let _: () = msg_send![typing_attributes,
                        setObject: ui_font
                        forKey: font_key
                    ];
                    let _: () = msg_send![typing_attributes,
                        setObject: paragraph_style
                        forKey: paragraph_key
                    ];
                    let _: () = msg_send![typing_attributes,
                        setObject: color
                        forKey: color_key
                    ];
                    let _: () = msg_send![editor,
                        setTypingAttributes: typing_attributes
                    ];
                    let _: () = msg_send![mutable_text, release];
                    let _: () = msg_send![paragraph_style, release];
                }
                let selection_start = params.selection_utf16.0.min(text_len);
                let selection_end = params.selection_utf16.1.min(text_len);
                let _: () = msg_send![editor,
                    setSelectedRange: NSRange {
                        location: selection_start,
                        length: selection_end.saturating_sub(selection_start),
                    }
                ];
                if is_first_responder == NO {
                    // Defer like `show_keyboard_with_type` so the call lands
                    // after UIKit finishes the current event pass.
                    let _: () = msg_send![editor,
                        performSelector: sel!(becomeFirstResponder)
                        withObject: ptr::null::<Object>()
                        afterDelay: 0.0_f64
                    ];
                }
            } else {
                // The editor is the source of truth while the takeover is
                // active; pull every committed mutation back into GPUI,
                // including during IME composition (`text` excludes marked
                // text, so committed state stays consistent).
                self.sync_editor_to_handler();
            }
        }
    }

    /// Restore the `GPUITextInputView` to its hidden 1x1 state.
    pub fn hide_native_editor(&self) {
        if !self.native_editor_active.replace(false) {
            return;
        }
        let editor = self.text_input_view;
        if editor.is_null() {
            return;
        }
        unsafe {
            // Final pull-sync before collapsing: any editor-side mutation that
            // never went through a per-frame refresh (IME deletes with an
            // active marked-text session, last-tap caret moves) must land in
            // the GPUI input handler, or blur will resurrect stale text.
            let marked: *mut Object = msg_send![editor, markedTextRange];
            if !marked.is_null() {
                // Force-commit the in-flight composition so `text` is final.
                let _: () = msg_send![editor, unmarkText];
            }
            if !self.sync_editor_to_handler() {
                // A GPUI dispatch still holds the handler borrow; retry once
                // the runloop settles so blur cannot resurrect stale text.
                let window_ptr: *mut IosWindow = self as *const IosWindow as *mut IosWindow;
                dispatch_on_main_after(
                    Duration::from_millis(50),
                    Box::new(move || unsafe {
                        let window = &*window_ptr;
                        if window.native_editor_active.get() {
                            window.sync_editor_to_handler();
                        }
                    }),
                );
            }
            let is_first_responder: BOOL = msg_send![editor, isFirstResponder];
            if is_first_responder == YES {
                // Defer like `hide_keyboard` — resigning synchronously out of
                // a GPUI event dispatch re-enters UIKit's end-editing flow,
                // which can synchronously call back into Rust input handlers.
                let _: () = msg_send![editor,
                    performSelector: sel!(resignFirstResponder)
                    withObject: ptr::null::<Object>()
                    afterDelay: 0.0_f64
                ];
            }
            let frame = core_graphics::geometry::CGRect {
                origin: core_graphics::geometry::CGPoint { x: 0.0, y: 0.0 },
                size: core_graphics::geometry::CGSize {
                    width: 1.0,
                    height: 1.0,
                },
            };
            let _: () = msg_send![editor, setFrame: frame];
            let _: () = msg_send![editor, setAlpha: 0.01_f64];
        }
    }

    /// Make the Metal view the first responder so attached iPad keyboards can
    /// send app-level key events when no text field is active.
    pub(super) fn focus_hardware_keyboard_view(&self) {
        if self.view.is_null() {
            return;
        }
        if let Some(last_show) = self.last_keyboard_show.get()
            && last_show.elapsed() < Duration::from_secs(1)
        {
            return;
        }

        unsafe {
            if !self.text_input_view.is_null() {
                let text_input_is_first_responder: BOOL =
                    msg_send![self.text_input_view, isFirstResponder];
                if text_input_is_first_responder == YES {
                    return;
                }
            }

            let _: () = msg_send![self.view,
                performSelector: sel!(becomeFirstResponder)
                withObject: ptr::null::<Object>()
                afterDelay: 0.0_f64
            ];
        }
    }

    /// Handle text input from the software keyboard
    pub fn handle_text_input(&self, text: *mut Object) {
        if text.is_null() {
            return;
        }

        if let Some(text_str) = ns_string_to_string(text) {
            log::info!("GPUI iOS: Text input: {:?}", text_str);

            // Try the global text input callback (for our TextInput components).
            // The text is captured in PENDING_TEXT regardless of whether we also
            // send key events below.
            let dispatched = crate::dispatch_text_input(&text_str);

            // Try the input handler (for GPUI's built-in text fields)
            if !dispatched
                && let Ok(mut handler) = self.input_handler.try_borrow_mut()
                && let Some(handler) = handler.as_mut()
            {
                handler.replace_text_in_range(None, &text_str);
                return;
            }

            // Send key events through GPUI's input callback.
            // Even if dispatch_text_input captured the text, we still send key
            // events so GPUI triggers a re-render cycle (which runs
            // drain_pending_text and updates the UI).
            //
            // We send the *entire* composed string as a single KeyDown event
            // rather than one event per codepoint, so grapheme clusters
            // (e.g. emoji with ZWJ, combining characters) stay intact.
            let keystroke = gpui::Keystroke {
                modifiers: Modifiers::default(),
                key: text_str.clone(),
                key_char: Some(text_str),
            };

            let event = PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: true,
            });

            self.dispatch_input(event);
        }
    }

    /// Mirror an in-progress UIKit marked-text session into GPUI.
    pub fn handle_marked_text(
        &self,
        text: *mut Object,
        selected_location: usize,
        selected_length: usize,
    ) {
        if text.is_null() {
            return;
        }
        if let Some(text) = ns_string_to_string(text)
            && let Ok(mut handler) = self.input_handler.try_borrow_mut()
            && let Some(handler) = handler.as_mut()
        {
            let selection = super::super::text_input::clamp_utf16_selection(
                &text,
                selected_location,
                selected_length,
            );
            handler.replace_and_mark_text_in_range(None, &text, Some(selection));
        }
    }

    /// Finish the active UIKit marked-text session.
    pub fn handle_unmark_text(&self) {
        if let Ok(mut handler) = self.input_handler.try_borrow_mut()
            && let Some(handler) = handler.as_mut()
        {
            handler.unmark_text();
        }
    }

    /// Handle the delete-backward action from the software keyboard.
    ///
    /// This is called by the `GPUITextInputView` when the user taps the
    /// backspace key.  We dispatch a special sentinel ("\x08") through the
    /// global text input callback so the active TextInput component can
    /// remove the last character.
    pub fn handle_delete_backward(&self) {
        log::info!("GPUI iOS: deleteBackward");

        // Try the global callback first (backspace = "\x08")
        crate::dispatch_text_input("\x08");

        // Always send a Backspace KeyDown event through GPUI to trigger
        // a re-render cycle (which runs drain_pending_text).
        let keystroke = gpui::Keystroke {
            modifiers: Modifiers::default(),
            key: "backspace".to_string(),
            key_char: None,
        };
        let event = PlatformInput::KeyDown(gpui::KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        });
        self.dispatch_input(event);
    }

    /// Handle a key event from an external keyboard
    pub fn handle_key_event(&self, key_code: u32, modifier_flags: u32, is_key_down: bool) {
        self.handle_key_event_with_characters(key_code, modifier_flags, None, is_key_down);
    }

    /// Handle a key event from an external keyboard, optionally using UIKit's
    /// layout-aware character value for printable input.
    pub(super) fn handle_key_event_with_characters(
        &self,
        key_code: u32,
        modifier_flags: u32,
        characters: Option<String>,
        is_key_down: bool,
    ) {
        use super::super::text_input::{
            key_code_to_key_down, key_code_to_key_down_with_characters, key_code_to_key_up,
            key_code_to_string, modifier_flags_to_modifiers,
        };

        let key = key_code_to_string(key_code);
        let modifiers = modifier_flags_to_modifiers(modifier_flags);
        self.modifiers.set(modifiers);

        if matches!(key_code, 0xE0..=0xE7) {
            return;
        }

        // While the native editor takeover is active the editor owns the
        // text; route printable hardware input and backspace through it so
        // the per-frame pull-sync does not clobber the edits.
        if self.native_editor_active.get()
            && is_key_down
            && !self.text_input_view.is_null()
        {
            let blocking_modifier =
                modifiers.control || modifiers.alt || modifiers.platform || modifiers.function;
            if key_code == 0x33 {
                unsafe {
                    let _: () = msg_send![self.text_input_view, deleteBackward];
                }
                return;
            }
            if !blocking_modifier
                && characters.as_ref().is_some_and(|chars| !chars.is_empty())
            {
                let chars = characters.unwrap_or_default();
                unsafe {
                    let ns_chars = crate::ios::ns_string_from_str(&chars);
                    let _: () = msg_send![self.text_input_view, insertText: ns_chars];
                }
                return;
            }
        }

        log::info!(
            "GPUI iOS: Key event - key: {:?}, modifiers: {:?}, down: {}",
            key,
            modifiers,
            is_key_down
        );

        // On key-down, dispatch cursor-movement control codes through the
        // global text input callback so TextField-based components receive them.
        if is_key_down {
            match key_code {
                0x50 => {
                    crate::dispatch_text_input("\x1b[D");
                } // Left arrow
                0x4F => {
                    crate::dispatch_text_input("\x1b[C");
                } // Right arrow
                0x4A => {
                    crate::dispatch_text_input("\x1b[H");
                } // Home
                0x4D => {
                    crate::dispatch_text_input("\x1b[F");
                } // End
                _ => {}
            }
        }

        let event = if is_key_down {
            if characters.is_some() {
                key_code_to_key_down_with_characters(key_code, modifier_flags, characters)
            } else {
                key_code_to_key_down(key_code, modifier_flags)
            }
        } else {
            key_code_to_key_up(key_code, modifier_flags)
        };

        self.dispatch_input(event);
    }

    /// Notify the window of active status changes (foreground/background).
    ///
    /// This is called by the FFI layer when the app transitions between
    /// foreground and background states.
    pub fn notify_active_status_change(&self, is_active: bool) {
        log::info!("GPUI iOS: Window active status changed to: {}", is_active);

        if let Some(callback) = self.active_status_callback.borrow_mut().as_mut() {
            callback(is_active);
        }
        // Touches never produce a real mouse-out, and backgrounding can pause
        // the app between a tap and its scheduled hover clear (or restore a
        // stale frame), leaving tapped buttons stuck in their hover style.
        // Clear hover on every foreground/background transition.
        self.hover_clear_scheduled.set(false);
        let _ = self.dispatch_input(PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position: gpui::point(gpui::px(-10000.0), gpui::px(-10000.0)),
            modifiers: gpui::Modifiers::default(),
            pressed_button: None,
        }));
        self.request_forced_frame();
        if is_active {
            self.focus_hardware_keyboard_view();
        }
    }

    /// Handle a layout change (e.g. rotation, split-screen resize).
    ///
    /// Called from `viewDidLayoutSubviews` on the GPUIViewController.
    /// Queries the current UIView bounds, updates the stored bounds/scale,
    /// reconfigures the Metal layer + wgpu surface, and fires the resize callback.
    pub fn handle_layout_change(&self) {
        let Some(metrics) = self.scene_metrics() else {
            return;
        };

        let new_w = metrics.width;
        let new_h = metrics.height;
        let new_scale = metrics.scale_factor;

        let old_bounds = self.bounds.get();
        let old_scale = self.scale_factor.get();

        let new_size = size(px(new_w), px(new_h));

        // Only process if something actually changed.
        if old_bounds.size == new_size && (old_scale - new_scale).abs() < 0.01 {
            return;
        }

        log::info!(
            "GPUI iOS: Layout changed — {:?} @{:.1}x → {:?} @{:.1}x ({:?})",
            old_bounds.size,
            old_scale,
            new_size,
            new_scale,
            metrics.layout_mode(),
        );

        // Update stored bounds (in logical pixels, matching GPUI convention).
        let new_bounds = Bounds {
            origin: Default::default(),
            size: new_size,
        };
        self.bounds.set(new_bounds);
        self.scale_factor.set(new_scale);
        crate::accessibility::update_accesskit_scale_factor(new_scale);

        unsafe {
            // Update the Metal layer's contentsScale so the drawable has the correct pixel dimensions.
            let layer: *mut Object = msg_send![self.view, layer];
            let scale = new_scale as core_graphics::base::CGFloat;
            let _: () = msg_send![layer, setContentsScale: scale];
        }

        // Update the wgpu renderer's surface configuration.
        let pixel_w = (new_w * new_scale).round().max(1.0) as i32;
        let pixel_h = (new_h * new_scale).round().max(1.0) as i32;
        {
            let mut guard = self.renderer.lock();
            if let Some(renderer) = guard.as_mut() {
                renderer.update_drawable_size(size(DevicePixels(pixel_w), DevicePixels(pixel_h)));
            }
        }

        // Fire the resize callback so GPUI re-layouts at the new size.
        let cb = self.resize_callback.borrow_mut().take();
        if let Some(mut cb) = cb {
            cb(new_size, new_scale);
            // Restore the callback for future resize events.
            let mut slot = self.resize_callback.borrow_mut();
            if slot.is_none() {
                *slot = Some(cb);
            }
        }
    }
}

impl HasWindowHandle for IosWindow {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError>
    {
        let view = NonNull::new(self.view as *mut c_void)
            .ok_or(raw_window_handle::HandleError::Unavailable)?;
        let handle = UiKitWindowHandle::new(view);
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(handle.into()) })
    }
}

impl HasDisplayHandle for IosWindow {
    fn display_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        let handle = UiKitDisplayHandle::new();
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(handle.into()) })
    }
}

impl PlatformWindow for IosWindow {
    fn visibility(&self) -> WindowVisibility {
        WindowVisibility::Visible
    }

    fn on_visibility_change(&self, _callback: Box<dyn FnMut(WindowVisibility)>) {}

    fn on_display_changed(&self, _callback: Box<dyn FnMut()>) {}

    fn bounds(&self) -> Bounds<Pixels> {
        self.bounds.get()
    }

    fn is_maximized(&self) -> bool {
        // Foreign-parent windows have no `UIWindow` and report windowed.
        !self.window.is_null()
    }

    fn window_bounds(&self) -> WindowBounds {
        if self.window.is_null() {
            WindowBounds::Windowed(self.bounds.get())
        } else {
            WindowBounds::Fullscreen(self.bounds.get())
        }
    }

    fn content_size(&self) -> Size<Pixels> {
        self.bounds.get().size
    }

    fn resize(&mut self, _size: Size<Pixels>) {
        // iOS windows cannot be resized programmatically
    }

    fn a11y_init(&self, callbacks: gpui::A11yCallbacks) {
        crate::accessibility::init_accesskit(callbacks, self.scale_factor.get());
    }

    fn a11y_tree_update(&self, tree_update: accesskit::TreeUpdate) {
        crate::accessibility::update_accesskit_tree(tree_update, self.scale_factor.get());
    }

    fn a11y_update_window_bounds(&self) {
        if !crate::accessibility::update_accesskit_scale_factor(self.scale_factor.get()) {
            crate::ios::ffi::gpui_ios_refresh_accessibility();
        }
    }

    fn scale_factor(&self) -> f32 {
        self.scale_factor.get()
    }

    fn appearance(&self) -> WindowAppearance {
        unsafe {
            let trait_collection: *mut Object = msg_send![self.view, traitCollection];
            let style: i64 = msg_send![trait_collection, userInterfaceStyle];
            match style {
                2 => WindowAppearance::Dark,
                _ => WindowAppearance::Light,
            }
        }
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(Rc::new(IosDisplay::main()))
    }

    fn mouse_position(&self) -> Point<Pixels> {
        self.mouse_position.get()
    }

    fn modifiers(&self) -> Modifiers {
        self.modifiers.get()
    }

    fn capslock(&self) -> Capslock {
        // Would need to check UIKeyModifierFlags
        Capslock { on: false }
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        *self.input_handler.borrow_mut() = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.input_handler.borrow_mut().take()
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        msg: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
    ) -> Option<futures::channel::oneshot::Receiver<usize>> {
        if self.window.is_null() {
            // No view controller to present from in extension contexts.
            return None;
        }
        let (tx, rx) = futures::channel::oneshot::channel();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));

        unsafe {
            // Create UIAlertController
            let alert_style: i64 = 1; // UIAlertControllerStyleAlert

            let title_str = super::super::ns_string_from_str(msg);
            let message_str = super::super::ns_string_from_str(detail.unwrap_or(""));

            let alert: *mut Object = msg_send![
                class!(UIAlertController),
                alertControllerWithTitle: title_str
                message: message_str
                preferredStyle: alert_style
            ];

            // Add buttons
            for (index, button) in answers.iter().enumerate() {
                let button_title = super::super::ns_string_from_str(button.label());

                let action_style: i64 = if button.is_cancel() { 1 } else { 0 }; // UIAlertActionStyleCancel or Default

                let tx_clone = tx.clone();
                let block = block2::RcBlock::new(move |_action: *mut c_void| {
                    if let Ok(mut guard) = tx_clone.lock()
                        && let Some(sender) = guard.take()
                    {
                        let _ = sender.send(index);
                    }
                });

                let action: *mut Object = msg_send![
                    class!(UIAlertAction),
                    actionWithTitle: button_title
                    style: action_style
                    handler: &*block
                ];

                let _: () = msg_send![alert, addAction: action];
            }

            // Present the alert
            let _: () = msg_send![
                self.view_controller,
                presentViewController: alert
                animated: YES
                completion: ptr::null::<Object>()
            ];
        }

        Some(rx)
    }

    fn activate(&self) {
        unsafe {
            let _: () = msg_send![self.window, makeKeyAndVisible];
        }
    }

    fn is_active(&self) -> bool {
        if self.window.is_null() {
            // Extension-safe: no `sharedApplication` access; the host
            // reports focus through the foreign FFI entry point.
            return self.foreign.is_active();
        }
        unsafe {
            let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
            let key_window: *mut Object = msg_send![app, keyWindow];
            self.window == key_window
        }
    }

    fn is_hovered(&self) -> bool {
        if self.window.is_null() {
            return self.foreign.is_hovered();
        }
        // Hover isn't really applicable on iOS
        false
    }

    fn set_title(&mut self, _title: &str) {
        // iOS apps don't have window titles
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        WindowBackgroundAppearance::Opaque
    }

    fn set_background_appearance(&self, _background_appearance: WindowBackgroundAppearance) {
        // Could adjust view background color
    }

    fn minimize(&self) {
        // iOS apps cannot be minimized
    }

    fn zoom(&self) {
        // iOS apps cannot be zoomed
    }

    fn toggle_fullscreen(&self) {
        // iOS apps are always fullscreen
    }

    fn is_fullscreen(&self) -> bool {
        !self.window.is_null()
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        *self.request_frame_callback.borrow_mut() = Some(callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        *self.input_callback.borrow_mut() = Some(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        if self.window.is_null() {
            // Foreign windows notify transition-only through the shared
            // cell; the host drives it via the foreign FFI entry point.
            self.foreign.on_active_change(callback);
        } else {
            *self.active_status_callback.borrow_mut() = Some(callback);
        }
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        if self.window.is_null() {
            self.foreign.on_hover_change(callback);
        } else {
            *self.hover_status_callback.borrow_mut() = Some(callback);
        }
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        *self.resize_callback.borrow_mut() = Some(callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        *self.moved_callback.borrow_mut() = Some(callback);
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        *self.should_close_callback.borrow_mut() = Some(callback);
    }

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        *self.hit_test_callback.borrow_mut() = Some(callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        *self.close_callback.borrow_mut() = Some(callback);
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        *self.appearance_changed_callback.borrow_mut() = Some(callback);
    }

    fn draw(&self, scene: &Scene) {
        // Applies a debounced foreign resize when one is due; no-op on
        // the app path.
        self.poll_foreign_surface_size();
        self.draw_scene(scene);
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.cached_sprite_atlas()
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        // iOS uses grayscale antialiasing, not subpixel rendering
        false
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        self.renderer_gpu_specs()
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {
        // iOS handles IME positioning automatically
    }
}

