use block::ConcreteBlock;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel, BOOL, YES};
use objc::{msg_send, sel, sel_impl, Encode, Encoding};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::macos_icon::{icon_bytes_for_system_appearance, image_from_icns};

const PROGRESS_CLASS: &str = "ReelsDockProgressView";
const BAR_INSET: f64 = 0.12;
const BAR_HEIGHT: f64 = 0.105;
const BAR_BOTTOM: f64 = 0.13;
const PROGRESS_STEPS: u32 = 1000;

#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

unsafe impl Encode for NSRect {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

static PROGRESS: AtomicU32 = AtomicU32::new(u32::MAX);
static VIEW: Mutex<Option<ViewRef>> = Mutex::new(Option::None);
static REGISTERED: OnceLock<&'static Class> = OnceLock::new();

struct ViewRef(*mut Object);

unsafe impl Send for ViewRef {}

struct Cleanup;

impl Drop for Cleanup {
    fn drop(&mut self) {
        set_progress(None);
    }
}

fn apply(progress: Option<f32>) {
    let state = progress
        .map(|value| (value.clamp(0.0, 1.0) * PROGRESS_STEPS as f32).round() as u32)
        .unwrap_or(u32::MAX);
    if PROGRESS.swap(state, Ordering::Relaxed) == state {
        return;
    }
    unsafe { apply_state(state) }
}

fn progress_class() -> &'static Class {
    *REGISTERED.get_or_init(|| unsafe {
        let superclass = class("NSView");
        let mut declaration = ClassDecl::new(PROGRESS_CLASS, superclass)
            .unwrap_or_else(|| panic!("failed to declare {PROGRESS_CLASS}"));
        declaration.add_ivar::<f64>("_reelsProgress");
        declaration.add_method(
            sel!(isFlipped),
            is_flipped as extern "C" fn(&Object, Sel) -> BOOL,
        );
        declaration.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(&Object, Sel, NSRect),
        );
        declaration.register()
    })
}

unsafe fn class(name: &str) -> &'static Class {
    Class::get(name).unwrap_or_else(|| panic!("missing Objective-C class: {name}"))
}

extern "C" fn is_flipped(_: &Object, _: Sel) -> BOOL {
    YES
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect {
        origin: NSPoint { x, y },
        size: NSSize { width, height },
    }
}

unsafe fn color(white: f64, alpha: f64) -> *mut Object {
    msg_send![class("NSColor"), colorWithCalibratedWhite:white alpha:alpha]
}

unsafe fn blue_color() -> *mut Object {
    msg_send![class("NSColor"), colorWithCalibratedRed:0.12f64 green:0.48f64 blue:1.0f64 alpha:1.0f64]
}

unsafe fn rounded_path(bounds: NSRect, radius: f64) -> *mut Object {
    msg_send![class("NSBezierPath"), bezierPathWithRoundedRect:bounds xRadius:radius yRadius:radius]
}

unsafe fn fill(path: *mut Object, fill_color: *mut Object) {
    let _: () = msg_send![fill_color, setFill];
    let _: () = msg_send![path, fill];
}

unsafe fn stroke(path: *mut Object, stroke_color: *mut Object, line_width: f64) {
    let _: () = msg_send![stroke_color, setStroke];
    let _: () = msg_send![path, setLineWidth:line_width];
    let _: () = msg_send![path, stroke];
}

extern "C" fn draw_rect(this: &Object, _: Sel, _dirty_rect: NSRect) {
    unsafe {
        let bounds: NSRect = msg_send![this, bounds];
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return;
        }
        if let Some(icon) = application_icon() {
            let _: () = msg_send![icon, drawInRect:bounds];
            let _: () = msg_send![icon, release];
        }
        let progress_ivar = this.get_ivar::<f64>("_reelsProgress");
        let progress = (*progress_ivar).clamp(0.0, 1.0);
        let tile_width = bounds.size.width;
        let tile_height = bounds.size.height;
        let track_width = tile_width * (1.0 - BAR_INSET * 2.0);
        let track_height = (tile_height * BAR_HEIGHT).clamp(5.0, 10.0);
        let track_y = tile_height - tile_height * BAR_BOTTOM - track_height;
        let track = rect(
            (tile_width - track_width) / 2.0,
            track_y,
            track_width,
            track_height,
        );
        let track_path = rounded_path(track, track_height / 2.0);
        fill(track_path, color(0.0, 0.22));
        stroke(track_path, color(1.0, 0.72), 1.0);

        let inset = 2.0_f64.min(track_height / 4.0);
        let inner_width = track_width - inset * 2.0;
        let inner_height = track_height - inset * 2.0;
        if progress > 0.0 && inner_width > 0.0 && inner_height > 0.0 {
            let fill_rect = rect(
                track.origin.x + inset,
                track.origin.y + inset,
                inner_width * progress,
                inner_height,
            );
            let fill_path = rounded_path(fill_rect, inner_height / 2.0);
            fill(fill_path, blue_color());
        }
    }
}

unsafe fn dock_tile() -> *mut Object {
    let application: *mut Object = msg_send![class("NSApplication"), sharedApplication];
    msg_send![application, dockTile]
}

unsafe fn application_icon() -> Option<*mut Object> {
    image_from_icns(icon_bytes_for_system_appearance())
}

unsafe fn ensure_view(tile: *mut Object) -> *mut Object {
    let mut view = VIEW.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(existing) = view.as_ref() {
        return existing.0;
    }
    let size: NSSize = msg_send![tile, size];
    let instance: *mut Object = msg_send![progress_class(), alloc];
    let instance: *mut Object =
        msg_send![instance, initWithFrame:rect(0.0, 0.0, size.width, size.height)];
    let _: () = msg_send![instance, setAutoresizingMask:18usize];
    let _: () = msg_send![tile, setContentView:instance];
    *view = Some(ViewRef(instance));
    instance
}

unsafe fn apply_state(state: u32) {
    let tile = dock_tile();
    if state == u32::MAX {
        let mut view = VIEW.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(existing) = view.take() {
            let _: () = msg_send![tile, setContentView:std::ptr::null_mut::<Object>()];
            let _: () = msg_send![existing.0, release];
        }
        crate::macos_icon::refresh_app_icon_without_progress();
        let _: () = msg_send![tile, display];
        return;
    }
    let instance = ensure_view(tile);
    let progress = state as f64 / PROGRESS_STEPS as f64;
    let progress_ivar = (&mut *instance).get_mut_ivar::<f64>("_reelsProgress");
    *progress_ivar = progress;
    let _: () = msg_send![tile, display];
}

pub fn set_progress(progress: Option<f32>) {
    unsafe {
        if !is_main_thread() {
            dispatch_to_main_thread(progress);
            return;
        };
        apply(progress);
    }
}

pub fn refresh_icon() {
    if PROGRESS.load(Ordering::Relaxed) == u32::MAX {
        return;
    }
    unsafe {
        if !is_main_thread() {
            let block = ConcreteBlock::new(refresh_icon_on_main_thread).copy();
            let block_ptr = &*block as *const _ as *mut c_void;
            let queue: *mut Object = msg_send![class("NSOperationQueue"), mainQueue];
            let operation: *mut Object =
                msg_send![class("NSBlockOperation"), blockOperationWithBlock:block_ptr];
            let _: () = msg_send![queue, addOperation:operation];
            std::mem::forget(block);
            return;
        }
        refresh_icon_on_main_thread();
    }
}

fn refresh_icon_on_main_thread() {
    let state = PROGRESS.load(Ordering::Relaxed);
    if state != u32::MAX {
        unsafe { apply_state(state) };
    }
}

pub fn install_cleanup() {
    static INSTALLED: OnceLock<Cleanup> = OnceLock::new();
    let _ = INSTALLED.get_or_init(|| Cleanup);
}

unsafe fn is_main_thread() -> bool {
    let is_main: BOOL = msg_send![class("NSThread"), isMainThread];
    is_main == YES
}

unsafe fn dispatch_to_main_thread(progress: Option<f32>) {
    let queue: *mut Object = msg_send![class("NSOperationQueue"), mainQueue];
    let block = ConcreteBlock::new(move || apply(progress)).copy();
    let block_ptr = &*block as *const _ as *mut c_void;
    let operation: *mut Object =
        msg_send![class("NSBlockOperation"), blockOperationWithBlock:block_ptr];
    let _: () = msg_send![queue, addOperation:operation];
    std::mem::forget(block);
}
