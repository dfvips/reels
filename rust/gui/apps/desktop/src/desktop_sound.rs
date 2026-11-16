#[cfg(any(target_os = "macos", target_os = "windows"))]
const FINISH_WAV: &[u8] = include_bytes!("../../../../assets/finish.wav");

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::{Mutex, OnceLock};

    use objc::runtime::{Class, Object};
    use objc::{msg_send, sel, sel_impl};

    use super::FINISH_WAV;

    static PLAYING_SOUND: OnceLock<Mutex<usize>> = OnceLock::new();

    unsafe fn class(name: &str) -> *const Class {
        Class::get(name).unwrap_or_else(|| panic!("missing Objective-C class: {name}"))
    }

    pub(super) fn play() {
        unsafe {
            let data: *mut Object = msg_send![class("NSData"), alloc];
            let data: *mut Object =
                msg_send![data, initWithBytes:FINISH_WAV.as_ptr() length:FINISH_WAV.len()];
            if data.is_null() {
                return;
            }
            let sound: *mut Object = msg_send![class("NSSound"), alloc];
            let sound: *mut Object = msg_send![sound, initWithData: data];
            let _: () = msg_send![data, release];
            if sound.is_null() {
                return;
            }
            let previous = {
                let playing = PLAYING_SOUND.get_or_init(|| Mutex::new(0));
                let mut playing = playing.lock().unwrap_or_else(|error| error.into_inner());
                let previous = *playing;
                *playing = sound as usize;
                previous
            };
            if previous != 0 {
                let previous = previous as *mut Object;
                let _: () = msg_send![previous, release];
            }
            let _: objc::runtime::BOOL = msg_send![sound, play];
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::ptr;

    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    use super::FINISH_WAV;

    const WAV_LEN: usize = include_bytes!("../../../../assets/finish.wav").len();

    #[repr(C, align(8))]
    struct AlignedWav([u8; WAV_LEN]);

    static ALIGNED_WAV: AlignedWav = {
        let mut bytes = [0; WAV_LEN];
        let mut index = 0;
        while index < WAV_LEN {
            bytes[index] = FINISH_WAV[index];
            index += 1;
        }
        AlignedWav(bytes)
    };

    pub(super) fn play() {
        unsafe {
            let _ = PlaySoundW(
                ALIGNED_WAV.0.as_ptr().cast::<u16>(),
                ptr::null_mut(),
                SND_ASYNC | SND_MEMORY | SND_NODEFAULT,
            );
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub(super) fn play() {}
}

pub fn play_finish_sound() {
    platform::play();
}
