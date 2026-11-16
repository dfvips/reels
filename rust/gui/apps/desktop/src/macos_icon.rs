use block::ConcreteBlock;
use objc::runtime::{Class, Object, BOOL, YES};
use objc::{msg_send, sel, sel_impl};
use std::os::raw::c_void;
use std::sync::OnceLock;

pub(crate) const LIGHT_ICON: &[u8] = include_bytes!("../../../../assets/logo/logo.icns");
pub(crate) const DARK_ICON: &[u8] = include_bytes!("../../../../assets/logo/logo-dark.icns");

pub fn set_app_icon_for_system_appearance() {
    register_refresh_observer();
    refresh_app_icon();
}

pub fn refresh_app_icon() {
    refresh_app_icon_without_progress();
    refresh_dock_progress();
}

pub(crate) fn refresh_app_icon_without_progress() {
    unsafe {
        let icon = icon_bytes_for_system_appearance();
        let Some(image) = image_from_icns(icon) else {
            return;
        };
        let application: *mut Object = msg_send![class("NSApplication"), sharedApplication];
        let _: () = msg_send![application, setApplicationIconImage: image];
        let _: () = msg_send![image, release];
    }
}

pub(crate) fn refresh_dock_progress() {
    super::macos_dock_progress::refresh_icon();
}

unsafe fn class(name: &str) -> *const Class {
    Class::get(name).unwrap_or_else(|| panic!("missing Objective-C class: {name}"))
}

unsafe fn ns_string(value: &str) -> *mut Object {
    let string: *mut Object = msg_send![class("NSString"), alloc];
    msg_send![string, initWithBytes:value.as_ptr() length:value.len() encoding:4usize]
}

unsafe fn standard_user_defaults() -> *mut Object {
    msg_send![class("NSUserDefaults"), standardUserDefaults]
}

pub(crate) unsafe fn icon_bytes_for_system_appearance() -> &'static [u8] {
    let defaults = standard_user_defaults();
    if defaults.is_null() {
        return LIGHT_ICON;
    }
    let key = ns_string("AppleInterfaceStyle");
    if key.is_null() {
        return LIGHT_ICON;
    }
    let style: *mut Object = msg_send![defaults, stringForKey: key];
    let _: () = msg_send![key, release];
    if style.is_null() {
        return LIGHT_ICON;
    }
    let dark_key = ns_string("Dark");
    if dark_key.is_null() {
        return LIGHT_ICON;
    }
    let is_dark: BOOL = msg_send![style, isEqualToString: dark_key];
    let _: () = msg_send![dark_key, release];
    if is_dark == YES {
        DARK_ICON
    } else {
        LIGHT_ICON
    }
}

pub(crate) unsafe fn image_from_icns(icon: &[u8]) -> Option<*mut Object> {
    let data: *mut Object = msg_send![class("NSData"), alloc];
    let data: *mut Object = msg_send![data, initWithBytes:icon.as_ptr() length:icon.len()];
    if data.is_null() {
        return None;
    }
    let image: *mut Object = msg_send![class("NSImage"), alloc];
    let image: *mut Object = msg_send![image, initWithData: data];
    let _: () = msg_send![data, release];
    if image.is_null() {
        return None;
    }
    Some(image)
}

fn register_refresh_observer() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    unsafe {
        if REGISTERED.get().is_some() {
            return;
        }
        REGISTERED.set(()).ok();
        let center: *mut Object = msg_send![class("NSNotificationCenter"), defaultCenter];
        let distributed_center: *mut Object =
            msg_send![class("NSDistributedNotificationCenter"), defaultCenter];
        let main_queue: *mut Object = msg_send![class("NSOperationQueue"), mainQueue];
        let block = ConcreteBlock::new(|_notification: *mut Object| refresh_app_icon()).copy();
        let block_ptr = &*block as *const _ as *mut c_void;
        let _: *mut Object = msg_send![distributed_center, addObserverForName:ns_string("AppleInterfaceStyleChangedNotification") object:std::ptr::null_mut::<Object>() queue:main_queue usingBlock:block_ptr];
        let _: *mut Object = msg_send![center, addObserverForName:ns_string("NSApplicationDidBecomeActiveNotification") object:std::ptr::null_mut::<Object>() queue:main_queue usingBlock:block_ptr];
        std::mem::forget(block);
    }
}
