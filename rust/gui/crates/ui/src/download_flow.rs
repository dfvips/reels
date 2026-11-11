//! 下载流程：确认、进度、完成总结、取消。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::{Context, WeakEntity};

use crate::download::{self, DownloadEvent};
use crate::paths::is_desktop;
use crate::{AppView, Dialog, DownloadUi, ToastKind};

impl AppView {
    pub fn request_download(&mut self, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            self.toast("没有可下载的视频链接", ToastKind::Error, cx);
            return;
        }
        if self.download.as_ref().is_some_and(|ui| !ui.finished) {
            self.toast("当前正在下载中，请等待完成", ToastKind::Error, cx);
            return;
        }
        self.dialog = Some(Dialog::DownloadConfirm);
        cx.notify();
    }

    pub fn confirm_download(&mut self, cx: &mut Context<Self>) {
        if !is_desktop() {
            self.begin_download(self.paths.default_download_dir.clone(), cx);
            return;
        }
        if self.settings.skip_location_prompt && !self.settings.last_directory.is_empty() {
            let directory = PathBuf::from(&self.settings.last_directory);
            self.begin_download(directory, cx);
            return;
        }
        // 原生目录选择框放在异步任务里执行：它自带的模态循环会阻塞
        // 主线程，若此时仍持有 App 借用，定时任务（toast 等）会触发
        // "RefCell already borrowed" panic。
        let initial = if !self.settings.last_directory.is_empty() {
            PathBuf::from(&self.settings.last_directory)
        } else {
            self.paths.default_download_dir.clone()
        };
        self.dialog = None;
        cx.notify();
        cx.spawn(async move |this: WeakEntity<AppView>, cx| {
            let picked = crate::home::pick_directory_blocking(Some(&initial));
            let _ = this.update(cx, |this, cx| match picked {
                Some(directory) => {
                    this.settings.last_directory = directory.display().to_string();
                    this.save_settings();
                    this.begin_download(directory, cx);
                }
                None => {
                    this.dialog = None;
                    this.toast("下载已取消", ToastKind::Info, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn begin_download(&mut self, directory: PathBuf, cx: &mut Context<Self>) {
        let urls: Vec<String> = self
            .results
            .iter()
            .flat_map(|item| item.entries.iter().map(|entry| entry.url.clone()))
            .collect();
        let titles: Vec<String> = self
            .results
            .iter()
            .flat_map(|item| std::iter::repeat(item.title.clone()).take(item.entries.len()))
            .collect();
        let extensions: Vec<Option<String>> = self
            .results
            .iter()
            .flat_map(|item| item.entries.iter().map(|entry| entry.extension.clone()))
            .collect();
        let total = urls.len();
        let ui = DownloadUi::new(total);
        let cancel = ui.cancel.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        crate::paths::ensure_dir(&self.paths.cache_dir);
        download::start_downloads(
            urls,
            titles,
            extensions,
            directory,
            self.paths.cache_dir.clone(),
            cancel,
            tx,
        );
        self.download = Some(ui);
        self.download_rx = Some(rx);
        self.dialog = Some(Dialog::DownloadProgress);
        cx.notify();

        cx.spawn(async move |this: WeakEntity<AppView>, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
            let finished = this
                .update(cx, |this, cx| {
                    let mut events = Vec::new();
                    if let Some(rx) = this.download_rx.as_ref() {
                        while let Ok(event) = rx.try_recv() {
                            events.push(event);
                        }
                    }
                    for event in events {
                        this.apply_download_event(event);
                    }
                    cx.notify();
                    this.download.as_ref().is_none_or(|ui| ui.finished)
                })
                .unwrap_or(true);
            if finished {
                let _ = this.update(cx, |this, cx| this.finish_download(cx));
                break;
            }
        })
        .detach();
    }

    fn apply_download_event(&mut self, event: DownloadEvent) {
        let Some(ui) = self.download.as_mut() else {
            return;
        };
        match event {
            DownloadEvent::Started { index, total } => {
                if let Some(slot) = ui.total_bytes.get_mut(index) {
                    *slot = total;
                }
                ui.current = index;
            }
            DownloadEvent::Progress {
                index,
                downloaded,
                total,
            } => {
                if let Some(slot) = ui.downloaded.get_mut(index) {
                    *slot = downloaded;
                }
                if total > 0 {
                    if let Some(slot) = ui.total_bytes.get_mut(index) {
                        *slot = total;
                    }
                }
                ui.current = index;
            }
            DownloadEvent::Completed { index, path } => {
                if let Some(slot) = ui.completed.get_mut(index) {
                    *slot = true;
                }
                if let Some(slot) = ui.total_bytes.get_mut(index) {
                    if *slot == 0 {
                        *slot = ui.downloaded.get(index).copied().unwrap_or(0);
                    }
                }
                ui.files.push(path);
                if index + 1 < ui.total {
                    ui.current = index + 1;
                }
            }
            DownloadEvent::Failed { index, error } => {
                ui.errors.push(error);
                if let Some(slot) = ui.completed.get_mut(index) {
                    *slot = true;
                }
                if index + 1 < ui.total {
                    ui.current = index + 1;
                }
            }
            DownloadEvent::Finished => {
                ui.finished = true;
            }
        }
    }

    fn finish_download(&mut self, cx: &mut Context<Self>) {
        self.download_rx = None;
        let Some(ui) = self.download.as_ref() else {
            self.dialog = None;
            return;
        };
        let success = ui.files.len();
        let total = ui.total;
        let files = ui.files.clone();
        let errors = ui.errors.clone();
        let cancelled = ui.cancel.load(Ordering::Relaxed);

        if success > 0 {
            self.toast(
                format!("批量下载完成: {success}/{total} 个视频"),
                ToastKind::Success,
                cx,
            );
        } else if cancelled {
            self.toast("下载已取消", ToastKind::Info, cx);
        } else if let Some(error) = errors.first() {
            self.toast(format!("下载失败: {error}"), ToastKind::Error, cx);
        } else {
            self.toast(format!("下载失败: 0/{total} 个视频"), ToastKind::Error, cx);
        }

        if is_desktop() && !files.is_empty() {
            self.dialog = Some(Dialog::DownloadSummary { files });
        } else {
            self.dialog = None;
        }
        cx.notify();
    }

    pub fn cancel_download(&mut self, cx: &mut Context<Self>) {
        if let Some(ui) = self.download.as_ref() {
            ui.cancel.store(true, Ordering::Relaxed);
        }
        self.dialog = None;
        self.toast("下载已取消", ToastKind::Info, cx);
    }
}
