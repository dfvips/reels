//! 下载流程：确认、进度、完成总结、取消。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::{Context, WeakEntity};

use douyin_core::extractor::{MediaEntry, MediaKind};

use crate::download::{self, DownloadEvent};
use crate::download_icon::report as report_download_icon_progress;
use crate::download_sound::play_finish_sound;
use crate::paths::is_desktop;
use crate::{AppView, Dialog, DownloadUi, ResourceScope, ResultItem, ToastKind};

pub(crate) fn has_multiple_video_qualities(item: &ResultItem) -> bool {
    // 图文笔记里的视频是各张实况图自带的，不算同一视频的画质选择。
    if item.media_kind == MediaKind::Image {
        return false;
    }
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

#[derive(Default)]
pub(crate) struct DownloadPlan {
    urls: Vec<String>,
    titles: Vec<String>,
    extensions: Vec<Option<String>>,
    live_videos: Vec<Option<String>>,
    skipped_audio: usize,
}

impl DownloadPlan {
    fn push(&mut self, title: &str, entry: &MediaEntry, live_video: Option<String>) {
        self.urls.push(entry.url.clone());
        self.titles.push(title.to_string());
        self.extensions.push(entry.extension.clone());
        self.live_videos.push(live_video);
    }
}

/// 单个媒体（或一组实况图 + 动轨）的下载计划。
pub(crate) fn build_single_plan(
    title: &str,
    entries: &[MediaEntry],
    live_mode: bool,
) -> DownloadPlan {
    let mut plan = DownloadPlan::default();
    let mut paired = vec![false; entries.len()];
    for (index, entry) in entries.iter().enumerate() {
        if paired[index] {
            continue;
        }
        let mut live_video = None;
        if live_mode && !ResultItem::is_video_entry(entry) {
            if let Some(next) = entries.get(index + 1) {
                if !paired[index + 1] && is_live_entry(next) {
                    paired[index + 1] = true;
                    live_video = Some(next.url.clone());
                }
            }
        }
        plan.push(title, entry, live_video);
    }
    plan
}

fn build_download_plan(
    results: &[ResultItem],
    scope: ResourceScope,
    live_mode: bool,
    skip_audio: bool,
) -> DownloadPlan {
    let mut plan = DownloadPlan::default();
    for item in results {
        let selected = item.select_entries(scope);
        let mut paired = vec![false; selected.len()];
        for (index, entry) in selected.iter().enumerate() {
            if paired[index] {
                continue;
            }
            // 相册不支持音频：手机端跳过背景音乐等音频条目。
            if skip_audio && is_audio_entry(entry) {
                plan.skipped_audio += 1;
                continue;
            }
            // 实况模式：图片与其后的 LIVE 动轨配对，动轨随照片封装，不再单独保存。
            let mut live_video = None;
            if live_mode && !ResultItem::is_video_entry(entry) {
                if let Some(next) = selected.get(index + 1) {
                    if !paired[index + 1] && is_live_entry(next) {
                        paired[index + 1] = true;
                        live_video = Some(next.url.clone());
                    }
                }
            }
            plan.push(&item.title, entry, live_video);
        }
    }
    plan
}

pub(crate) fn is_audio_entry(entry: &MediaEntry) -> bool {
    let extension = entry
        .extension
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "mp3" | "m4a" | "aac" | "wav" | "flac") {
        return true;
    }
    let path = entry.url.split('?').next().unwrap_or(&entry.url);
    path.to_ascii_lowercase()
        .rsplit('.')
        .next()
        .is_some_and(|ext| matches!(ext, "mp3" | "m4a" | "aac" | "wav" | "flac"))
}

/// 抖音与小红书的实况动轨都紧跟其图片并标记 `LIVE`。
pub(crate) fn is_live_entry(entry: &MediaEntry) -> bool {
    entry
        .quality
        .as_deref()
        .is_some_and(|quality| quality.trim().eq_ignore_ascii_case("LIVE"))
}

#[cfg(test)]
mod tests {
    use douyin_core::extractor::{MediaEntry, MediaKind, MediaPlatform};

    use super::{build_download_plan, has_multiple_video_qualities};
    use crate::{ResourceScope, ResultItem};

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
            covers: Vec::new(),
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

    #[test]
    fn image_note_live_videos_are_not_quality_choices() {
        let mut image_item = item(vec![video(Some("原画")), video(Some("LIVE"))]);
        image_item.media_kind = MediaKind::Image;
        assert!(!has_multiple_video_qualities(&image_item));
    }

    fn image_note(entries: Vec<MediaEntry>) -> ResultItem {
        let mut image_item = item(entries);
        image_item.media_kind = MediaKind::Image;
        image_item
    }

    fn image(url: &str) -> MediaEntry {
        MediaEntry {
            url: url.to_string(),
            extension: Some("jpg".to_string()),
            ..Default::default()
        }
    }

    fn audio() -> MediaEntry {
        MediaEntry {
            url: "https://example.com/music.mp3".to_string(),
            extension: Some("mp3".to_string()),
            quality: Some("背景音乐".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn mobile_skips_audio_entries() {
        let plan = build_download_plan(
            &[image_note(vec![
                image("https://example.com/a.jpg"),
                audio(),
            ])],
            ResourceScope::All,
            false,
            true,
        );
        assert_eq!(plan.urls.len(), 1);
        assert_eq!(plan.skipped_audio, 1);
    }

    #[test]
    fn desktop_keeps_audio_entries() {
        let plan = build_download_plan(
            &[image_note(vec![
                image("https://example.com/a.jpg"),
                audio(),
            ])],
            ResourceScope::All,
            false,
            false,
        );
        assert_eq!(plan.urls.len(), 2);
        assert_eq!(plan.skipped_audio, 0);
    }

    #[test]
    fn live_mode_pairs_image_with_following_live_video() {
        let plan = build_download_plan(
            &[image_note(vec![
                image("https://example.com/a.jpg"),
                video(Some("LIVE")),
                image("https://example.com/b.jpg"),
            ])],
            ResourceScope::All,
            true,
            true,
        );
        assert_eq!(plan.urls.len(), 2);
        assert_eq!(
            plan.live_videos[0].as_deref(),
            Some("https://example.com/video.mp4")
        );
        assert_eq!(plan.live_videos[1], None);
    }

    #[test]
    fn non_live_mode_keeps_motion_videos_separate() {
        let plan = build_download_plan(
            &[image_note(vec![
                image("https://example.com/a.jpg"),
                video(Some("LIVE")),
            ])],
            ResourceScope::All,
            false,
            true,
        );
        assert_eq!(plan.urls.len(), 2);
        assert!(plan.live_videos.iter().all(Option::is_none));
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
        let plan = build_download_plan(
            &self.results,
            self.download_scope,
            crate::supports_live_photo_export() && self.settings.live_photo_export,
            !is_desktop(),
        );
        self.confirm_download_plan(plan, cx);
    }

    /// 体验版单条资源下载：复用统一的目录选择与进度流程。
    pub fn confirm_single_download(
        &mut self,
        title: &str,
        entries: Vec<MediaEntry>,
        live_mode: bool,
        cx: &mut Context<Self>,
    ) {
        if self.download.as_ref().is_some_and(|ui| !ui.finished) {
            self.toast("当前正在下载中，请等待完成", ToastKind::Error, cx);
            return;
        }
        let plan = build_single_plan(title, &entries, live_mode);
        self.confirm_download_plan(plan, cx);
    }

    pub(crate) fn confirm_download_plan(&mut self, plan: DownloadPlan, cx: &mut Context<Self>) {
        if !is_desktop() {
            self.begin_download_plan(plan, self.paths.default_download_dir.clone(), cx);
            return;
        }
        if self.settings.skip_location_prompt && !self.settings.last_directory.is_empty() {
            let directory = PathBuf::from(&self.settings.last_directory);
            self.begin_download_plan(plan, directory, cx);
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
                    this.begin_download_plan(plan, directory, cx);
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

    fn begin_download_plan(
        &mut self,
        plan: DownloadPlan,
        directory: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let total = plan.urls.len();
        if total == 0 {
            report_download_icon_progress(None);
            if !is_desktop() {
                let message = if plan.skipped_audio > 0 {
                    format!(
                        "已跳过 {} 个 MP3 文件，没有可保存到相册的资源",
                        plan.skipped_audio
                    )
                } else {
                    "没有可保存到相册的资源".to_string()
                };
                self.toast(message, ToastKind::Info, cx);
            }
            return;
        }
        let mut ui = DownloadUi::new(total);
        ui.skipped_audio = plan.skipped_audio;
        let cancel = ui.cancel.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        crate::paths::ensure_dir(&self.paths.cache_dir);
        download::start_downloads(
            plan.urls,
            plan.titles,
            plan.extensions,
            plan.live_videos,
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
            DownloadEvent::Failed {
                index,
                error,
                timed_out,
            } => {
                if timed_out {
                    ui.timed_out += 1;
                }
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
        let skipped_audio = ui.skipped_audio;
        let timed_out = ui.timed_out;
        let cancelled = ui.cancel.load(Ordering::Relaxed);
        let failed = errors.len().min(total.saturating_sub(success));
        let timeout_note = if timed_out > 0 {
            format!("，跳过 {timed_out} 个超时链接")
        } else {
            String::new()
        };

        if is_desktop() {
            if success > 0 {
                if !cancelled {
                    play_finish_sound();
                }
                self.toast(
                    format!("批量下载完成: {success}/{total} 个视频{timeout_note}"),
                    ToastKind::Success,
                    cx,
                );
            } else if cancelled {
                self.toast("下载已取消", ToastKind::Info, cx);
            } else if timed_out > 0 && timed_out == errors.len() {
                self.toast(
                    format!("{timed_out} 个链接超过 10 秒无响应，已跳过"),
                    ToastKind::Error,
                    cx,
                );
            } else if let Some(error) = errors.first() {
                self.toast(
                    format!("下载失败: {error}{timeout_note}"),
                    ToastKind::Error,
                    cx,
                );
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
                skipped: skipped_audio,
                timed_out,
            });
        } else if cancelled {
            self.toast("下载已取消", ToastKind::Info, cx);
            self.dialog = None;
        } else if timed_out > 0 && timed_out == errors.len() {
            self.toast(
                format!("{timed_out} 个链接超过 10 秒无响应，已跳过"),
                ToastKind::Error,
                cx,
            );
            self.dialog = None;
        } else if let Some(error) = errors.first() {
            let message = if skipped_audio > 0 {
                format!("下载失败: {error}（已跳过 {skipped_audio} 个 MP3 文件）")
            } else {
                format!("下载失败: {error}")
            };
            self.toast(message, ToastKind::Error, cx);
            self.dialog = None;
        } else if skipped_audio > 0 {
            let message = if timed_out > 0 {
                format!(
                    "下载失败: 0/{total} 个视频（已跳过 {skipped_audio} 个 MP3 文件、{timed_out} 个超时链接）"
                )
            } else {
                format!("下载失败: 0/{total} 个视频（已跳过 {skipped_audio} 个 MP3 文件）")
            };
            self.toast(message, ToastKind::Error, cx);
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
