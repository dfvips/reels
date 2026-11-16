use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

pub type DownloadIconProgressHook = dyn Fn(Option<f32>) + Send + Sync;

static HOOK: OnceLock<Box<DownloadIconProgressHook>> = OnceLock::new();
static VALUE: AtomicU32 = AtomicU32::new(u32::MAX);
static REDRAW: Mutex<()> = Mutex::new(());

pub fn set_download_icon_progress_hook(hook: Option<Box<DownloadIconProgressHook>>) {
    if let Some(hook) = hook {
        let _ = HOOK.set(hook);
    }
}

pub(crate) fn report(progress: Option<f32>) {
    let Some(hook) = HOOK.get() else {
        return;
    };
    let quantized = progress.map(|value| (value.clamp(0.0, 1.0) * 1000.0).round() as u32);
    let state = quantized.unwrap_or(u32::MAX);
    if VALUE.swap(state, Ordering::Relaxed) == state {
        return;
    }
    let _guard = REDRAW.lock();
    if VALUE.load(Ordering::Relaxed) != state {
        return;
    }
    hook(quantized.map(|value| value as f32 / 1000.0));
}
