#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::ptr;

use douyin_ui::IdmLink;
use windows_sys::core::{GUID, HRESULT, PWSTR};
use windows_sys::Win32::Foundation::{SysAllocString, SysFreeString};
use windows_sys::Win32::Globalization::LOCALE_USER_DEFAULT;
use windows_sys::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER,
    DISPATCH_METHOD, DISPPARAMS,
};
use windows_sys::Win32::System::Variant::{
    VariantInit, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR, VT_I4,
};

const IID_NULL: GUID = GUID::from_u128(0);
const IID_IDISPATCH: GUID = GUID::from_u128(0x00020400_0000_0000_c000_000000000046);
const IDM_FLAG_NO_CONFIRMATION: i32 = 1;
const COINIT_APARTMENTTHREADED: u32 = 2;
const RPC_E_CHANGED_MODE: HRESULT = 0x80010106_u32 as HRESULT;
const S_FALSE: HRESULT = 1;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[repr(C)]
struct IdmDispatch {
    vtable: *const IdmDispatchVtable,
}

#[repr(C)]
struct IdmDispatchVtable {
    query_interface:
        unsafe extern "system" fn(*mut IdmDispatch, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut IdmDispatch) -> u32,
    release: unsafe extern "system" fn(*mut IdmDispatch) -> u32,
    get_type_info_count: unsafe extern "system" fn(*mut IdmDispatch, *mut u32) -> HRESULT,
    get_type_info:
        unsafe extern "system" fn(*mut IdmDispatch, u32, u32, *mut *mut c_void) -> HRESULT,
    get_ids_of_names: unsafe extern "system" fn(
        *mut IdmDispatch,
        *const GUID,
        *mut PWSTR,
        u32,
        u32,
        *mut i32,
    ) -> HRESULT,
    invoke: unsafe extern "system" fn(
        *mut IdmDispatch,
        i32,
        *const GUID,
        u32,
        u16,
        *mut DISPPARAMS,
        *mut VARIANT,
        *mut c_void,
        *mut u32,
    ) -> HRESULT,
}

struct ComScope {
    initialized: bool,
}

impl ComScope {
    fn new() -> Result<Self, String> {
        let status = unsafe { CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED) };
        if status < 0 && status != RPC_E_CHANGED_MODE && status != S_FALSE {
            return Err(format!("CoInitializeEx 失败: {status:#x}"));
        }
        Ok(Self {
            initialized: status >= 0,
        })
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

struct Bstr(*const u16);

impl Bstr {
    fn new(value: &str) -> Result<Self, String> {
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bstr = unsafe { SysAllocString(wide.as_ptr()) };
        if bstr.is_null() {
            Err(format!("无法分配 BSTR: {value}"))
        } else {
            Ok(Self(bstr))
        }
    }

    fn empty() -> Self {
        Self(ptr::null())
    }

    fn as_ptr(&self) -> *const u16 {
        self.0
    }
}

impl Drop for Bstr {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { SysFreeString(self.0) };
        }
    }
}

fn empty_variant() -> VARIANT {
    let mut variant: VARIANT = unsafe { std::mem::zeroed() };
    unsafe { VariantInit(&mut variant) };
    variant
}

fn variant_str(value: &str) -> Result<(Bstr, VARIANT), String> {
    let bstr = Bstr::new(value)?;
    let variant = VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: VARIANT_0_0 {
                vt: VT_BSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 {
                    bstrVal: bstr.as_ptr(),
                },
            },
        },
    };
    Ok((bstr, variant))
}

fn variant_i4(value: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: value },
            },
        },
    }
}

fn check(status: HRESULT, operation: &str) -> Result<(), String> {
    if status < 0 {
        Err(format!("{operation} 失败: {status:#x}"))
    } else {
        Ok(())
    }
}

fn get_dispid(dispatch: *mut IdmDispatch) -> Result<i32, String> {
    let mut method_name: Vec<u16> = "SendLinkToIDM2"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut names = [method_name.as_mut_ptr()];
    let mut dispid = 0;
    let get_ids_of_names = unsafe { (*(*dispatch).vtable).get_ids_of_names };
    check(
        unsafe {
            get_ids_of_names(
                dispatch,
                &IID_NULL,
                names.as_mut_ptr(),
                1,
                LOCALE_USER_DEFAULT,
                &mut dispid,
            )
        },
        "查找 SendLinkToIDM2",
    )?;
    Ok(dispid)
}

fn send_one_link(dispatch: *mut IdmDispatch, dispid: i32, link: &IdmLink) -> Result<(), String> {
    let (url, url_variant) = variant_str(&link.url)?;
    let (referer, referer_variant) = variant_str(&link.referer)?;
    let (user_agent, user_agent_variant) = variant_str(USER_AGENT)?;
    let (file_name, file_name_variant) = variant_str(&link.file_name)?;
    let (cookie, cookie_variant) = match link.cookie.as_deref() {
        Some(value) if !value.is_empty() => variant_str(value)?,
        _ => (Bstr::empty(), empty_variant()),
    };
    let (post_data, post_data_variant) = (Bstr::empty(), empty_variant());
    let (user, user_variant) = (Bstr::empty(), empty_variant());
    let (password, password_variant) = (Bstr::empty(), empty_variant());
    let (path, path_variant) = (Bstr::empty(), empty_variant());
    let reserved2 = empty_variant();
    let flags = variant_i4(IDM_FLAG_NO_CONFIRMATION);

    let mut arguments = vec![
        url_variant,
        referer_variant,
        cookie_variant,
        post_data_variant,
        user_variant,
        password_variant,
        path_variant,
        file_name_variant,
        flags,
        user_agent_variant,
        reserved2,
    ];
    arguments.reverse();

    let mut parameters = DISPPARAMS {
        rgvarg: arguments.as_mut_ptr(),
        rgdispidNamedArgs: ptr::null_mut(),
        cArgs: arguments.len() as u32,
        cNamedArgs: 0,
    };
    let mut result = empty_variant();
    let mut argument_error = 0;
    let invoke = unsafe { (*(*dispatch).vtable).invoke };
    let status = unsafe {
        invoke(
            dispatch,
            dispid,
            &IID_NULL,
            LOCALE_USER_DEFAULT,
            DISPATCH_METHOD,
            &mut parameters,
            &mut result,
            ptr::null_mut(),
            &mut argument_error,
        )
    };
    check(status, "SendLinkToIDM2")?;
    drop(url);
    drop(referer);
    drop(user_agent);
    drop(file_name);
    drop(cookie);
    drop(post_data);
    drop(user);
    drop(password);
    drop(path);
    Ok(())
}

pub fn send_links(links: &[IdmLink]) -> Result<(), String> {
    let _com = ComScope::new()?;
    let prog_id: Vec<u16> = "IDMan.CIDMLinkTransmitter"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut class_id = GUID::from_u128(0);
    check(
        unsafe { CLSIDFromProgID(prog_id.as_ptr(), &mut class_id) },
        "查找 IDM COM 组件",
    )?;
    let mut unknown = ptr::null_mut();
    check(
        unsafe {
            CoCreateInstance(
                &class_id,
                ptr::null_mut(),
                CLSCTX_LOCAL_SERVER,
                &IID_IDISPATCH,
                &mut unknown,
            )
        },
        "唤醒 IDM",
    )?;
    let dispatch = unknown as *mut IdmDispatch;
    let mut result = get_dispid(dispatch).and_then(|dispid| {
        for link in links {
            send_one_link(dispatch, dispid, link)?;
        }
        Ok(())
    });
    if result.is_ok() && links.is_empty() {
        result = Err("没有可导出的内容".to_string());
    }
    let release = unsafe { (*(*dispatch).vtable).release };
    unsafe { release(dispatch) };
    result
}
