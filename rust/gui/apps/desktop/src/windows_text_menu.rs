#![cfg(target_os = "windows")]

use std::ptr;

use douyin_ui::{TextContextMenuAction, TextContextMenuRequest};
use windows_sys::Win32::Foundation::{GetLastError, POINT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow,
    SetForegroundWindow, TrackPopupMenu, HMENU, MF_GRAYED, MF_SEPARATOR, MF_STRING, TPM_LEFTALIGN,
    TPM_RETURNCMD, TPM_RIGHTBUTTON,
};

const MENU_CUT: usize = 1;
const MENU_COPY: usize = 2;
const MENU_PASTE: usize = 3;
const MENU_SELECT_ALL: usize = 4;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn add_item(menu: HMENU, id: usize, label: &str, enabled: bool) {
    let text = wide(label);
    let flags = MF_STRING | if enabled { 0 } else { MF_GRAYED };
    unsafe {
        let _ = AppendMenuW(menu, flags, id, text.as_ptr());
    }
}

fn command_for(request: &TextContextMenuRequest) -> Option<TextContextMenuAction> {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return None;
    }

    add_item(
        menu,
        MENU_CUT,
        "剪切(&T)",
        request.has_selection && !request.password,
    );
    add_item(
        menu,
        MENU_COPY,
        "复制(&C)",
        request.has_selection && !request.password,
    );
    add_item(menu, MENU_PASTE, "粘贴(&P)", request.can_paste);
    unsafe {
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
    }
    add_item(menu, MENU_SELECT_ALL, "全选(&A)", true);

    let mut cursor = POINT { x: 0, y: 0 };
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    let selected = unsafe {
        let window = GetForegroundWindow();
        let _ = SetForegroundWindow(window);
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_LEFTALIGN | TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            0,
            window,
            ptr::null(),
        )
    };
    let action = match selected as usize {
        MENU_CUT if request.has_selection && !request.password => Some(TextContextMenuAction::Cut),
        MENU_COPY if request.has_selection && !request.password => {
            Some(TextContextMenuAction::Copy)
        }
        MENU_PASTE if request.can_paste => Some(TextContextMenuAction::Paste),
        MENU_SELECT_ALL => Some(TextContextMenuAction::SelectAll),
        _ => None,
    };
    unsafe {
        let _ = DestroyMenu(menu);
        let _ = GetLastError();
    }
    action
}

pub fn install() {
    let hook: douyin_ui::TextContextMenuHook = Box::new(command_for);
    douyin_ui::set_text_context_menu_hook(Some(hook));
}
