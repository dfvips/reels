#![cfg(target_os = "macos")]

use dispatch::ffi::{dispatch_async_f, dispatch_get_main_queue, dispatch_queue_t};
use douyin_ui::{MacTextContextMenuRequest, MacTextContextMenuRequestHook, TextContextMenuAction};
use gpui::{App, AppCell};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel, NO, YES};
use objc::{msg_send, sel, sel_impl, Encode, Encoding};
use std::cell::Cell;
use std::ffi::c_void;
use std::ptr;
use std::rc::Rc;

const MENU_CLASS: &str = "ReelsTextContextMenuTarget";
const MENU_CUT: isize = 101;
const MENU_COPY: isize = 102;
const MENU_PASTE: isize = 103;
const MENU_SELECT_ALL: isize = 104;

#[repr(C)]
#[derive(Clone, Copy)]
struct NSPoint {
    x: f64,
    y: f64,
}

unsafe impl Encode for NSPoint {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGPoint=dd}") }
    }
}

struct DeferredRequest {
    request: MacTextContextMenuRequest,
}

struct DeferredAction {
    input_id: String,
    action: TextContextMenuAction,
}

thread_local! {
    static APP_CELL: Cell<Option<Rc<AppCell>>> = const { Cell::new(None) };
    static SELECTED_ACTION: Cell<isize> = const { Cell::new(0) };
}

unsafe fn class(name: &str) -> &'static Class {
    Class::get(name).unwrap_or_else(|| panic!("missing Objective-C class: {name}"))
}

unsafe fn ns_string(value: &str) -> *mut Object {
    let string: *mut Object = msg_send![class("NSString"), alloc];
    msg_send![string, initWithBytes:value.as_ptr() length:value.len() encoding:4usize]
}

unsafe fn menu_location() -> NSPoint {
    msg_send![class("NSEvent"), mouseLocation]
}

fn app_cell() -> Option<Rc<AppCell>> {
    APP_CELL.with(|cell| cell.replace(None))
}

fn menu_target_class() -> &'static Class {
    static REGISTERED: std::sync::OnceLock<&'static Class> = std::sync::OnceLock::new();
    *REGISTERED.get_or_init(|| unsafe {
        let superclass = class("NSObject");
        let mut declaration = ClassDecl::new(MENU_CLASS, superclass)
            .unwrap_or_else(|| panic!("failed to declare {MENU_CLASS}"));
        declaration.add_method(
            sel!(reelsCut:),
            menu_action as extern "C" fn(&Object, Sel, *mut Object),
        );
        declaration.add_method(
            sel!(reelsCopy:),
            menu_action as extern "C" fn(&Object, Sel, *mut Object),
        );
        declaration.add_method(
            sel!(reelsPaste:),
            menu_action as extern "C" fn(&Object, Sel, *mut Object),
        );
        declaration.add_method(
            sel!(reelsSelectAll:),
            menu_action as extern "C" fn(&Object, Sel, *mut Object),
        );
        declaration.register()
    })
}

extern "C" fn menu_action(_: &Object, _: Sel, item: *mut Object) {
    unsafe {
        let tag: isize = msg_send![item, tag];
        SELECTED_ACTION.with(|selected| selected.set(tag));
    }
}

unsafe fn add_item(
    menu: *mut Object,
    target: *mut Object,
    title: &str,
    selector: Sel,
    tag: isize,
    enabled: bool,
) {
    let title = ns_string(title);
    let key = ns_string("");
    let item: *mut Object = msg_send![class("NSMenuItem"), alloc];
    let item: *mut Object = msg_send![item, initWithTitle:title action:selector keyEquivalent:key];
    let _: () = msg_send![title, release];
    let _: () = msg_send![key, release];
    let _: () = msg_send![item, setTarget: target];
    let _: () = msg_send![item, setTag: tag];
    let _: () = msg_send![item, setEnabled: if enabled { YES } else { NO }];
    let _: () = msg_send![menu, addItem: item];
    let _: () = msg_send![item, release];
}

unsafe fn show_context_menu(request: &MacTextContextMenuRequest) {
    SELECTED_ACTION.with(|selected| selected.set(0));

    let menu: *mut Object = msg_send![class("NSMenu"), alloc];
    let menu: *mut Object = msg_send![menu, init];
    let _: () = msg_send![menu, setAutoenablesItems: NO];

    let target: *mut Object = msg_send![menu_target_class(), alloc];
    let target: *mut Object = msg_send![target, init];

    add_item(
        menu,
        target,
        "剪切",
        sel!(reelsCut:),
        MENU_CUT,
        request.has_selection && !request.password,
    );
    add_item(
        menu,
        target,
        "复制",
        sel!(reelsCopy:),
        MENU_COPY,
        request.has_selection && !request.password,
    );
    add_item(
        menu,
        target,
        "粘贴",
        sel!(reelsPaste:),
        MENU_PASTE,
        request.can_paste,
    );
    let separator: *mut Object = msg_send![class("NSMenuItem"), separatorItem];
    let _: () = msg_send![menu, addItem: separator];
    add_item(
        menu,
        target,
        "全选",
        sel!(reelsSelectAll:),
        MENU_SELECT_ALL,
        true,
    );

    let location = menu_location();
    let _: () = msg_send![menu, popUpMenuPositioningItem:ptr::null_mut::<Object>() atLocation:location inView:ptr::null_mut::<Object>()];
    let selected = SELECTED_ACTION.with(|selected| selected.get());
    let _: () = msg_send![target, release];
    let _: () = msg_send![menu, release];

    let action = match selected {
        MENU_CUT => TextContextMenuAction::Cut,
        MENU_COPY => TextContextMenuAction::Copy,
        MENU_PASTE => TextContextMenuAction::Paste,
        MENU_SELECT_ALL => TextContextMenuAction::SelectAll,
        _ => return,
    };

    let context = Box::into_raw(Box::new(DeferredAction {
        input_id: request.input_id.clone(),
        action,
    })) as *mut c_void;
    let queue: dispatch_queue_t = dispatch_get_main_queue();
    dispatch_async_f(queue, context, apply_deferred_action);
}

extern "C" fn show_deferred_context_menu(context: *mut c_void) {
    let deferred = unsafe { Box::from_raw(context as *mut DeferredRequest) };
    unsafe { show_context_menu(&deferred.request) };
}

extern "C" fn apply_deferred_action(context: *mut c_void) {
    let deferred = unsafe { Box::from_raw(context as *mut DeferredAction) };
    let DeferredAction { input_id, action } = *deferred;
    let Some(app) = app_cell() else {
        return;
    };
    {
        if let Ok(mut cx) = app.try_borrow_mut() {
            let app: &mut App = &mut cx;
            douyin_ui::dispatch_mac_text_context_menu_action(&input_id, action, app);
        }
    }
    APP_CELL.with(|cell| cell.set(Some(app)));
}

fn queue_context_menu(request: &MacTextContextMenuRequest) {
    let context = Box::into_raw(Box::new(DeferredRequest {
        request: request.clone(),
    })) as *mut c_void;
    let queue: dispatch_queue_t = dispatch_get_main_queue();
    unsafe { dispatch_async_f(queue, context, show_deferred_context_menu) };
}

pub fn install(app: Rc<AppCell>) {
    APP_CELL.with(|cell| cell.set(Some(app)));
    let hook: MacTextContextMenuRequestHook = Rc::new(|request| queue_context_menu(request));
    douyin_ui::set_mac_text_context_menu_request_hook(Some(hook));
}
