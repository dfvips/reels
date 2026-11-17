#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::ptr;

use douyin_ui::IdmLink;
use windows_sys::core::{GUID, HRESULT};
use windows_sys::Win32::Foundation::{SysAllocString, SysFreeString};
use windows_sys::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER,
};
use windows_sys::Win32::System::Variant::{
    VariantInit, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR,
};

const CLSID_CIDM_LINK_TRANSMITTER: GUID = GUID::from_u128(0xac746233_e9d3_49cd_862f_068f7b7ccca4);
const IID_ICIDM_LINK_TRANSMITTER2: GUID = GUID::from_u128(0x94d09862_1875_4fc9_b434_91cf25c840a1);
const IDM_FLAG_NO_CONFIRMATION: i32 = 2;
const COINIT_APARTMENTTHREADED: u32 = 2;
const RPC_E_CHANGED_MODE: HRESULT = 0x80010106_u32 as HRESULT;
const S_FALSE: HRESULT = 1;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[repr(C)]
struct CidmLinkTransmitter2 {
    vtable: *const CidmLinkTransmitter2Vtable,
}

#[repr(C)]
struct CidmLinkTransmitter2Vtable {
    query_interface: unsafe extern "system" fn(
        *mut CidmLinkTransmitter2,
        *const GUID,
        *mut *mut c_void,
    ) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut CidmLinkTransmitter2) -> u32,
    release: unsafe extern "system" fn(*mut CidmLinkTransmitter2) -> u32,
    send_link_to_idm: unsafe extern "system" fn(
        *mut CidmLinkTransmitter2,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        i32,
    ) -> HRESULT,
    send_link_to_idm2: unsafe extern "system" fn(
        *mut CidmLinkTransmitter2,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        i32,
        VARIANT,
        VARIANT,
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

fn bstr_variant(value: &Bstr) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: VARIANT_0_0 {
                vt: VT_BSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 {
                    bstrVal: value.as_ptr(),
                },
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

fn send_one_link(transmitter: *mut CidmLinkTransmitter2, link: &IdmLink) -> Result<(), String> {
    let url = Bstr::new(&link.url)?;
    let referer = Bstr::new(&link.referer)?;
    let cookie = Bstr::new(link.cookie.as_deref().unwrap_or_default())?;
    let data = Bstr::new("")?;
    let user = Bstr::new("")?;
    let password = Bstr::new("")?;
    let local_path = Bstr::new("")?;
    let file_name = Bstr::new(&link.file_name)?;
    let user_agent = Bstr::new(USER_AGENT)?;
    let user_agent_variant = bstr_variant(&user_agent);
    let reserved2 = empty_variant();

    let send_link_to_idm2 = unsafe { (*(*transmitter).vtable).send_link_to_idm2 };
    check(
        unsafe {
            send_link_to_idm2(
                transmitter,
                url.as_ptr(),
                referer.as_ptr(),
                cookie.as_ptr(),
                data.as_ptr(),
                user.as_ptr(),
                password.as_ptr(),
                local_path.as_ptr(),
                file_name.as_ptr(),
                IDM_FLAG_NO_CONFIRMATION,
                user_agent_variant,
                reserved2,
            )
        },
        "发送到 IDM",
    )
}

pub fn send_links(links: &[IdmLink]) -> Result<(), String> {
    if links.is_empty() {
        return Err("没有可导出的内容".to_string());
    }
    let _com = ComScope::new()?;
    let prog_id: Vec<u16> = "IDMan.CIDMLinkTransmitter"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut class_id = CLSID_CIDM_LINK_TRANSMITTER;
    let _ = unsafe { CLSIDFromProgID(prog_id.as_ptr(), &mut class_id) };
    let mut unknown = ptr::null_mut();
    check(
        unsafe {
            CoCreateInstance(
                &class_id,
                ptr::null_mut(),
                CLSCTX_LOCAL_SERVER,
                &IID_ICIDM_LINK_TRANSMITTER2,
                &mut unknown,
            )
        },
        "唤醒 IDM",
    )?;
    let transmitter = unknown as *mut CidmLinkTransmitter2;
    let mut result = Ok(());
    for link in links {
        result = send_one_link(transmitter, link);
        if result.is_err() {
            break;
        }
    }
    let release = unsafe { (*(*transmitter).vtable).release };
    unsafe { release(transmitter) };
    result
}
