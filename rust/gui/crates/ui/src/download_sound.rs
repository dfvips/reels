use std::sync::OnceLock;

pub type DownloadSoundHook = dyn Fn() + Send + Sync;

static HOOK: OnceLock<Box<DownloadSoundHook>> = OnceLock::new();

pub fn set_download_sound_hook(hook: Option<Box<DownloadSoundHook>>) {
    if let Some(hook) = hook {
        let _ = HOOK.set(hook);
    }
}

pub(crate) fn play_finish_sound() {
    if let Some(hook) = HOOK.get() {
        hook();
    }
}
