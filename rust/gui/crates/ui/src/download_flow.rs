//! 下载流程：确认、进度、完成总结、取消。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::{Context, WeakEntity};

use crate::download::{self, DownloadEvent};
use crate::download_icon::report as report_download_icon_progress;
use crate::download_sound::play_finish_sound;
use crate::paths::is_desktop;
use crate::{AppView, Dialog, DownloadUi, ResourceScope, ResultItem, ToastKind};

pub(crate) fn has_multiple_video_qualities(item: &ResultItem) -> bool {
    let mut qualities = std::collections::HashSet::new();
    for entry in item
        .entries
        .iter()
        .filter(|entry| ResultItem::is_video_entry(entry))
    {
        let Some(quality) = entry
            .quality
            .as_deref()
            .map(str::trim)
            .filter(|quality| !quality.is_empty())
        else {
            continue;
        };
        qualities.insert(quality.to_ascii_lowercase());
        if qualities.len() > 1 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use douyin_core::extractor::{MediaEntry, MediaKind, MediaPlatform};

    use super::has_multiple_video_qualities;
    use crate::ResultItem;

    fn video(quality: Option<&str>) -> MediaEntry {
        MediaEntry {
            url: "https://example.com/video.mp4".to_string(),
            extension: Some("mp4".to_string()),
            quality: quality.map(str::to_string),
            ..Default::default()
        }
    }

    fn item(entries: Vec<MediaEntry>) -> ResultItem {
        ResultItem {
            title: "test".to_string(),
            platform: MediaPlatform::Douyin,
            media_kind: MediaKind::Video,
            entries,
        }
    }

    #[test]
    fn repeated_video_quality_is_not_a_choice() {
        assert!(!has_multiple_video_qualities(&item(vec![
            video(Some("原画")),
            video(Some("原画")),
        ])));
    }

    #[test]
    fn distinct_video_qualities_are_a_choice() {
        assert!(has_multiple_video_qualities(&item(vec![
            video(Some("原画")),
            video(Some("720P")),
        ])));
    }

    #[test]
    fn missing_quality_is_ignored() {
        assert!(!has_multiple_video_qualities(&item(vec![
            video(Some("720P")),
            video(None),
        ])));
    }
}

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
        let has_quality_choice = self.results.iter().any(has_multiple_video_qualities);
        if has_quality_choice {
            self.dialog = Some(Dialog::DownloadSelection);
            cx.notify();
            return;
        }
        self.confirm_download(ResourceScope::All, cx);
        cx.notify();
    }

    pub fn confirm_download(&mut self, scope: ResourceScope, cx: &mut Context<Self>) {
        self.download_scope = scope;
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
        let mut urls = Vec::new();
        let mut titles = Vec::new();
        let mut extensions = Vec::new();
        for item in &self.results {
            let selected = item.select_entries(self.download_scope);
            for entry in selected {
                urls.push(entry.url.clone());
                titles.push(item.title.clone());
                extensions.push(entry.extension.clone());
            }
        }
        let total = urls.len();
        if total == 0 {
            report_download_icon_progress(None);
            return;
        }
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
        report_download_icon_progress(Some(0.0));
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
                    let progress = this
                        .download
                        .as_ref()
                        .map(|ui| ui.overall_progress())
                        .unwrap_or(1.0);
                    report_download_icon_progress(Some(progress));
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
        report_download_icon_progress(None);
        let Some(ui) = self.download.as_ref() else {
            self.dialog = None;
            return;
        };
        let success = ui.files.len();
        let total = ui.total;
        let files = ui.files.clone();
        let errors = ui.errors.clone();
        let cancelled = ui.cancel.load(Ordering::Relaxed);
        let failed = errors.len().min(total.saturating_sub(success));

        if is_desktop() {
            if success > 0 {
                if !cancelled {
                    play_finish_sound();
                }
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
            self.dialog = (!files.is_empty()).then(|| Dialog::DownloadSummary { files });
        } else if !files.is_empty() {
            // 移动端只要有成功保存的文件就只弹“已保存”弹窗，
            // 失败数量并入弹窗正文，不再叠加失败轻提示。
            self.dialog = Some(Dialog::DownloadSaved {
                count: files.len(),
                failed,
            });
        } else if cancelled {
            self.toast("下载已取消", ToastKind::Info, cx);
            self.dialog = None;
        } else if let Some(error) = errors.first() {
            self.toast(format!("下载失败: {error}"), ToastKind::Error, cx);
            self.dialog = None;
        } else {
            self.toast(format!("下载失败: 0/{total} 个视频"), ToastKind::Error, cx);
            self.dialog = None;
        }
        cx.notify();
    }

    pub fn cancel_download(&mut self, cx: &mut Context<Self>) {
        report_download_icon_progress(None);
        if let Some(ui) = self.download.as_ref() {
            ui.cancel.store(true, Ordering::Relaxed);
        }
        self.dialog = None;
        self.toast("下载已取消", ToastKind::Info, cx);
    }
}
