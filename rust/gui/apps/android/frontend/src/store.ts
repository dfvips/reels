import { computed, nextTick, reactive, watch } from "vue";
import {
  commands,
  type ExtractedResult_Serialize,
  type MediaItem_Serialize,
  type Settings,
  type WindowInsets,
} from "./bindings";
import { unwrap } from "./ipc";

export type Route = "home" | "results" | "settings";
export type ResultView = "geek" | "simple";
export type DialogKind =
  | "help"
  | "about"
  | "cookie"
  | "live"
  | "cache"
  | "no-cache"
  | "cache-result"
  | "download-selection"
  | "download-progress"
  | "download-saved";
export type ExperienceTab = "all" | "title";
export type SlideKind = "video" | "image" | "live" | "audio" | "cover";

export interface SlideSpec {
  kind: SlideKind;
  entryIndex: number;
  liveIndex: number | null;
  coverIndex: number | null;
  qualityIndices: number[];
}

interface TransitionState {
  from: Route;
  to: Route;
  kind: "push" | "pop";
  offset: number;
  animate: boolean;
}

interface ToastState {
  message: string;
  kind: "info" | "success" | "error";
  seq: number;
}

interface DownloadProgress {
  total: number;
  current: number;
  completed: number;
  currentProgress: number;
  overall: number;
  downloaded: number;
  totalSize: number;
  currentSize: number;
  currentTotal: number;
  skippedAudio: number;
  timedOut: number;
  errors: string[];
  success: number;
  cancelled: boolean;
}

interface DownloadPlanEntry {
  url: string;
  title: string;
  extension: string | null;
  liveUrl: string | null;
  sizeBytes: number;
}

interface DownloadSavedState {
  count: number;
  failed: number;
  skipped: number;
  timedOut: number;
}

interface ClearResultState {
  success: boolean;
  message: string;
  details: string[];
}

const defaultSettings: Settings = {
  themeMode: "system",
  lastDirectory: "",
  skipLocationPrompt: false,
  rememberLocation: false,
  saveToGallery: true,
  xhsCookie: "",
  livePhotoExport: true,
  resultView: "geek",
};

const systemTheme = matchMedia("(prefers-color-scheme: dark)");

export const state = reactive({
  settings: { ...defaultSettings },
  systemDark: systemTheme.matches,
  route: "home" as Route,
  transition: null as TransitionState | null,
  input: "",
  homeInputFocused: false,
  results: [] as ExtractedResult_Serialize[],
  loading: false,
  toast: null as ToastState | null,
  dialog: null as DialogKind | null,
  dialogBusy: false,
  expTab: "all" as ExperienceTab,
  expItem: 0,
  expSlide: 0,
  expSlideOffset: 0,
  expSlideAnimating: false,
  expQualityMenu: false,
  qualityByItem: {} as Record<number, number>,
  cacheInfo: { size: 0, path: "" } as { size: number; path: string },
  cachePathCount: 0,
  clearResult: {
    success: true,
    message: "缓存已清理",
    details: ["当前剩余缓存: 0 B"],
  } as ClearResultState,
  download: null as DownloadProgress | null,
  downloadSaved: { count: 0, failed: 0, skipped: 0, timedOut: 0 } as DownloadSavedState,
  cookieDraft: "",
  cookieError: false,
  liveChoice: true,
  version: "1.0.6",
  backAt: 0,
  insets: { left: 0, top: 0, right: 0, bottom: 0, imeBottom: 0 } as WindowInsets,
  viewportWidth: window.innerWidth,
  viewportHeight: window.innerHeight,
  visualHeight: window.visualViewport?.height ?? window.innerHeight,
  visualOffset: window.visualViewport?.offsetTop ?? 0,
  thumbnails: {} as Record<string, { status: "loading" | "ready" | "failed"; data: string }>,
});

let toastSeq = 0;
let toastTimer = 0;
let transitionTimer = 0;
let viewTransitionRunning = false;
let initialized = false;
let insetTimer = 0;
let homeInsetTimer = 0;
let downloading = false;
let initPromise: Promise<void> | null = null;
let dialogCloseGuardUntil = 0;

export const isDark = computed(
  () =>
    state.settings.themeMode === "dark" ||
    (state.settings.themeMode === "system" && state.systemDark),
);

export const isLandscape = computed(
  () => state.viewportWidth > Math.max(state.visualHeight, 1),
);

export const compactHome = computed(
  () =>
    state.route === "home" &&
    state.homeInputFocused &&
    (state.insets.imeBottom || 0) > 96 &&
    state.viewportHeight -
      (state.insets.imeBottom || 0) -
      (state.insets.top || 0) <
      260,
);

export const themeIcon = computed(() =>
  state.settings.themeMode === "light"
    ? "SUN"
    : state.settings.themeMode === "dark"
      ? "MOON"
      : "CONTRAST",
);

export const themeModeName = computed(() => {
  if (state.settings.themeMode === "light") return "浅色模式";
  if (state.settings.themeMode === "dark") return "深色模式";
  return "跟随系统";
});

export const currentItem = computed(() => state.results[state.expItem] ?? null);

export const dialogCardStyle = computed(() => {
  const width = Math.min(
    Math.max(state.viewportWidth - 48, 1),
    Math.max(state.viewportWidth * 0.8, 1),
    480,
  );
  const visibleHeight =
    state.dialog === "cookie" && !isLandscape.value
      ? Math.max(
          state.viewportHeight - (state.insets.imeBottom || 0),
          1,
        )
      : Math.max(state.viewportHeight, 1);
  const maxHeight =
    state.dialog === "cookie" && isLandscape.value
      ? Math.max(visibleHeight - 8, 1)
      : Math.max(visibleHeight * 0.8, 1);
  const style: Record<string, string> = {
    width: `${width}px`,
    maxHeight: `${maxHeight}px`,
  };
  return style;
});

export function formatSize(value: number | null | undefined): string {
  if (!value) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let amount = value;
  let index = 0;
  while (amount >= 1024 && index < units.length - 1) {
    amount /= 1024;
    index += 1;
  }
  return index === 0 ? `${Math.round(value)} B` : `${amount.toFixed(1)} ${units[index]}`;
}

export function formatDuration(value: number | null | undefined): string {
  const total = Math.max(0, Math.floor(value || 0));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes >= 60) {
    return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}:${String(
      seconds,
    ).padStart(2, "0")}`;
  }
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export function entryMeta(entry: MediaItem_Serialize): string {
  const parts: string[] = [];
  if (entry.quality?.trim()) parts.push(entry.quality);
  if (entry.sizeBytes) parts.push(formatSize(entry.sizeBytes));
  if (entry.width && entry.height) parts.push(`${entry.width}x${entry.height}`);
  if (entry.durationSecs) parts.push(`时长 ${formatDuration(entry.durationSecs)}`);
  return parts.join(" · ");
}

export function isVideoEntry(entry: MediaItem_Serialize): boolean {
  const extension = (entry.extension || "").toLowerCase();
  const url = entry.url.toLowerCase();
  return (
    ["mp4", "m4v", "mov", "webm"].includes(extension) ||
    /\.(mp4|m4v|mov|webm)$/.test(url) ||
    url.includes("/video/") ||
    (url.includes("/stream/") && url.includes(".mp4"))
  );
}

export function isAudioEntry(entry: MediaItem_Serialize): boolean {
  const extension = (entry.extension || "").toLowerCase();
  if (["mp3", "m4a", "aac", "wav", "flac"].includes(extension)) return true;
  const path = entry.url.split("?")[0].toLowerCase();
  return /\.(mp3|m4a|aac|wav|flac)$/.test(path);
}

export function isLiveEntry(entry: MediaItem_Serialize): boolean {
  return (entry.quality || "").trim().toLowerCase() === "live";
}

export function slidesForItem(item: ExtractedResult_Serialize | null): SlideSpec[] {
  if (!item) return [];
  if (item.mediaKind === "video") {
    const qualityIndices = item.entries
      .map((entry, index) => ({ entry, index }))
      .filter(
        ({ entry }) =>
          isVideoEntry(entry) && !isAudioEntry(entry) && !isLiveEntry(entry),
      )
      .map(({ index }) => index);
    if (!qualityIndices.length) return [];
    const slides: SlideSpec[] = item.covers.map((_, coverIndex) => ({
      kind: "cover",
      entryIndex: 0,
      liveIndex: null,
      coverIndex,
      qualityIndices: [],
    }));
    slides.push({
      kind: "video",
      entryIndex: qualityIndices[0],
      liveIndex: null,
      coverIndex: null,
      qualityIndices,
    });
    return slides;
  }

  const slides: SlideSpec[] = [];
  let index = 0;
  while (index < item.entries.length) {
    const entry = item.entries[index];
    if (isAudioEntry(entry)) {
      slides.push({
        kind: "audio",
        entryIndex: index,
        liveIndex: null,
        coverIndex: null,
        qualityIndices: [index],
      });
      index += 1;
      continue;
    }
    if (isLiveEntry(entry)) {
      slides.push({
        kind: "video",
        entryIndex: index,
        liveIndex: null,
        coverIndex: null,
        qualityIndices: [index],
      });
      index += 1;
      continue;
    }
    const live =
      item.entries[index + 1] && isLiveEntry(item.entries[index + 1])
        ? index + 1
        : null;
    slides.push({
      kind: live === null ? "image" : "live",
      entryIndex: index,
      liveIndex: live,
      coverIndex: null,
      qualityIndices: [index],
    });
    index += live === null ? 1 : 2;
  }
  return slides;
}

export function slideBaseEntry(
  item: ExtractedResult_Serialize,
  slide: SlideSpec,
): MediaItem_Serialize | null {
  return slide.kind === "cover"
    ? (item.covers[slide.coverIndex ?? 0] ?? null)
    : (item.entries[slide.entryIndex] ?? null);
}

export function selectedQualityIndices(slide: SlideSpec): number[] {
  return slide.qualityIndices.length
    ? slide.qualityIndices
    : [slide.entryIndex];
}

export function selectedEntryForSlide(
  item: ExtractedResult_Serialize | null,
  slide: SlideSpec | null,
): MediaItem_Serialize | null {
  if (!item || !slide) return null;
  const base = slideBaseEntry(item, slide);
  if (slide.kind === "cover") return base;
  const indices = selectedQualityIndices(slide);
  const selectedEntryIndex = state.qualityByItem[state.expItem];
  const selectedPosition = Math.max(
    0,
    indices.findIndex((entryIndex) => entryIndex === selectedEntryIndex),
  );
  return item.entries[indices[selectedPosition] ?? indices[0]] ?? base;
}

export function previewUrlFor(
  item: ExtractedResult_Serialize,
  slide: SlideSpec,
): string | null {
  const base = slideBaseEntry(item, slide);
  const selected = selectedEntryForSlide(item, slide);
  if (slide.kind === "audio") return null;
  if (slide.kind === "cover") {
    return selected?.thumbnailUrl || selected?.url || null;
  }
  if (slide.kind === "image" || slide.kind === "live") {
    return (
      base?.thumbnailUrl ||
      base?.coverUrl ||
      base?.url ||
      null
    );
  }
  return selected?.thumbnailUrl || selected?.coverUrl || null;
}

export function ensureThumbnail(url: string | null): void {
  if (!url || state.thumbnails[url]) return;
  state.thumbnails[url] = { status: "loading", data: "" };
  unwrap(commands.thumbnailData(url))
    .then((data) => {
      state.thumbnails[url] = { status: "ready", data };
    })
    .catch(() => {
      state.thumbnails[url] = { status: "failed", data: "" };
    });
}

export function retryThumbnails(urls: Array<string | null>): void {
  for (const url of urls) {
    if (url) delete state.thumbnails[url];
  }
  for (const url of urls) ensureThumbnail(url);
}

export function showToast(
  message: string,
  kind: ToastState["kind"] = "info",
): void {
  toastSeq += 1;
  const seq = toastSeq;
  state.toast = { message, kind, seq };
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    if (state.toast?.seq === seq) state.toast = null;
  }, 1800);
}

function blurInput(): void {
  const active = document.activeElement;
  if (active instanceof HTMLElement) active.blur();
}

export function saveSettings(): void {
  void unwrap(commands.saveSettings(state.settings)).catch(() => undefined);
}

export function cycleTheme(): void {
  state.settings.themeMode =
    state.settings.themeMode === "system"
      ? "light"
      : state.settings.themeMode === "light"
        ? "dark"
        : "system";
  saveSettings();
  showToast(`已切换为${themeModeName.value}`);
}

export function navigateTo(to: Route): void {
  if (state.transition || viewTransitionRunning || to === state.route) return;
  blurInput();
  if (
    runViewTransition("push", () => {
      state.route = to;
    })
  ) {
    return;
  }
  const from = state.route;
  state.route = to;
  state.transition = {
    from,
    to,
    kind: "push",
    offset: window.innerWidth,
    animate: false,
  };
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      if (!state.transition) return;
      state.transition.animate = true;
      state.transition.offset = 0;
    });
  });
  window.clearTimeout(transitionTimer);
  transitionTimer = window.setTimeout(() => {
    state.transition = null;
  }, 300);
}

export function navigateBack(): void {
  if (state.transition || viewTransitionRunning) return;
  const active = document.activeElement;
  if (
    active instanceof HTMLTextAreaElement ||
    active instanceof HTMLInputElement
  ) {
    active.blur();
    return;
  }
  if (state.dialog) {
    closeDialog();
    return;
  }
  if (state.route !== "home") {
    blurInput();
    if (
      runViewTransition("pop", () => {
        state.route = "home";
      })
    ) {
      return;
    }
    const from = state.route;
    state.route = "home";
    state.transition = {
      from,
      to: "home",
      kind: "pop",
      offset: 0,
      animate: false,
    };
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (!state.transition) return;
        state.transition.animate = true;
        state.transition.offset = window.innerWidth;
      });
    });
    window.clearTimeout(transitionTimer);
    transitionTimer = window.setTimeout(() => {
      state.transition = null;
    }, 300);
    return;
  }
  const now = Date.now();
  if (now - state.backAt <= 2000) {
    state.backAt = 0;
    void unwrap(commands.exitApp()).catch(() => undefined);
  } else {
    state.backAt = now;
    blurInput();
    showToast("再按一次退出");
  }
}

type ViewTransitionLike = {
  finished: Promise<void>;
};

type ViewTransitionDocument = Document & {
  startViewTransition?: (
    callback: () => void | Promise<void>,
  ) => ViewTransitionLike;
};

function runViewTransition(
  kind: "push" | "pop",
  update: () => void,
): boolean {
  const start = (document as ViewTransitionDocument).startViewTransition;
  if (typeof start !== "function") return false;
  const root = document.documentElement;
  root.dataset.pageTransition = kind;
  viewTransitionRunning = true;
  const transition = start.call(document, async () => {
    update();
    await nextTick();
  });
  const finish = () => {
    viewTransitionRunning = false;
    delete root.dataset.pageTransition;
  };
  transition.finished.then(finish, finish);
  return true;
}

export async function pasteFromClipboard(): Promise<void> {
  try {
    state.input = await unwrap(commands.clipboardRead());
  } catch {
    showToast("无法读取剪贴板", "error");
  }
}

export function clearInput(): void {
  state.input = "";
}

async function writeClipboard(text: string): Promise<void> {
  await unwrap(commands.clipboardWrite(text));
}

export async function copyAll(): Promise<void> {
  if (!state.results.length) {
    showToast("没有可复制的内容", "error");
    return;
  }
  const text = state.results
    .flatMap((item) => [item.title || "", ...item.entries.map((entry) => entry.url)])
    .join("\n");
  await writeClipboard(text);
  showToast("已复制所有链接到剪贴板");
}

export async function copyEntry(entry: MediaItem_Serialize): Promise<void> {
  await writeClipboard(entry.url);
  showToast("已复制链接", "success");
}

export async function parseLinks(): Promise<void> {
  if (!state.input.trim()) {
    showToast("请输入短视频链接", "error");
    return;
  }
  blurInput();
  state.loading = true;
  state.results = [];
  closeDialog();
  try {
    const raw = await unwrap(commands.parseLinks(state.input));
    const items: ExtractedResult_Serialize[] = [];
    const errors: string[] = [];
    for (const result of raw) {
      if (result.success) {
        if (!result.entries.length) continue;
        const fallback =
          result.platform === "xhs"
            ? `小红书笔记_${items.length + 1}`
            : result.mediaKind === "image"
              ? `抖音图文_${items.length + 1}`
              : `抖音视频_${items.length + 1}`;
        items.push({
          ...result,
          title: result.title?.trim() ? result.title : fallback,
        });
      } else if (result.error) {
        errors.push(result.error);
      }
    }
    if (!items.length) {
      showToast(
        errors.length
          ? `解析失败: ${errors.join(", ")}`
          : "未找到有效的视频链接",
        "error",
      );
      return;
    }
    state.results = items;
    state.expItem = 0;
    state.expTab = "all";
    state.expSlide = 0;
    state.expSlideOffset = 0;
    state.expQualityMenu = false;
    state.qualityByItem = {};
    const count = items.reduce((sum, item) => sum + item.entries.length, 0);
    showToast(`成功解析 ${count} 个链接`);
    navigateTo("results");
  } catch (error) {
    showToast(String(error), "error");
  } finally {
    state.loading = false;
  }
}

export function toggleResultView(): void {
  state.settings.resultView =
    state.settings.resultView === "simple" ? "geek" : "simple";
  state.expQualityMenu = false;
  state.expTab = "all";
  state.expSlide = 0;
  state.expSlideOffset = 0;
  saveSettings();
}

export async function openSettings(): Promise<void> {
  await refreshCacheInfo();
  navigateTo("settings");
}

export async function refreshCacheInfo(): Promise<void> {
  try {
    const info = await unwrap(commands.cacheInfo());
    state.cacheInfo = { size: info.size || 0, path: info.path };
    state.cachePathCount = state.cacheInfo.size > 0 ? 1 : 0;
  } catch {
    state.cacheInfo = { size: 0, path: "" };
    state.cachePathCount = 0;
  }
}

export function openDialog(kind: DialogKind): void {
  blurInput();
  state.cookieError = false;
  state.expQualityMenu = false;
  if (kind === "live") state.liveChoice = state.settings.livePhotoExport;
  if (kind === "cookie") {
    state.cookieDraft = state.settings.xhsCookie;
    // Do not inherit a stale IME inset from the previous dialog session.
    state.insets.imeBottom = 0;
  }
  state.dialog = kind;
}

export async function openClearCache(): Promise<void> {
  await refreshCacheInfo();
  state.dialog = state.cacheInfo.size > 0 ? "cache" : "no-cache";
}

export function closeDialog(): void {
  blurInput();
  dialogCloseGuardUntil = performance.now() + 350;
  if (state.dialog === "cookie") {
    state.cookieDraft = state.settings.xhsCookie;
    state.cookieError = false;
    state.insets.imeBottom = 0;
    window.setTimeout(() => void refreshInsets(), 80);
  }
  state.dialog = null;
}

export function cycleThemePlaceholder(): void {
  cycleTheme();
}

function isValidCookieHeader(value: string): boolean {
  const input = value.trim();
  if (!input) return true;
  if ([...input].some((character) => character.charCodeAt(0) < 0x20 || character.charCodeAt(0) === 0x7f)) {
    return false;
  }
  let pairs = 0;
  for (const segment of input.split(";")) {
    const trimmed = segment.trim();
    if (!trimmed) continue;
    const separator = trimmed.indexOf("=");
    if (separator < 0) return false;
    const name = trimmed.slice(0, separator).trim();
    if (!name || ![...name].every((character) => /[A-Za-z0-9!#$%&'*+\-.^_`|~]/.test(character))) {
      return false;
    }
    pairs += 1;
  }
  return pairs > 0;
}

export function saveCookie(): void {
  const cookie = state.cookieDraft.trim();
  if (!isValidCookieHeader(cookie)) {
    state.cookieError = true;
    return;
  }
  state.settings.xhsCookie = cookie;
  saveSettings();
  closeDialog();
  showToast("Cookie 设置已保存", "success");
}

export function saveLiveChoice(): void {
  state.settings.livePhotoExport = state.liveChoice;
  saveSettings();
  closeDialog();
  showToast(
    state.liveChoice
      ? "保存到相册时将封装为 Motion Photo"
      : "保存到相册时将保存为图片和视频",
  );
}

export async function clearCache(): Promise<void> {
  state.dialogBusy = true;
  try {
    await unwrap(commands.clearCache());
    await refreshCacheInfo();
    state.clearResult = {
      success: true,
      message: "缓存已清理",
      details: [`当前剩余缓存: ${formatSize(state.cacheInfo.size)}`],
    };
    state.dialog = "cache-result";
  } catch (error) {
    state.clearResult = {
      success: false,
      message: "清除缓存失败:",
      details: [String(error), `当前剩余缓存: ${formatSize(state.cacheInfo.size)}`],
    };
    state.dialog = "cache-result";
  } finally {
    state.dialogBusy = false;
  }
}

function hasMultipleVideoQualities(item: ExtractedResult_Serialize): boolean {
  if (item.mediaKind !== "video") return false;
  const qualities = new Set(
    item.entries
      .filter(isVideoEntry)
      .map((entry) => entry.quality?.trim().toLowerCase())
      .filter((quality): quality is string => Boolean(quality)),
  );
  return qualities.size > 1;
}

function selectEntries(
  item: ExtractedResult_Serialize,
  scope: "all" | "original",
): MediaItem_Serialize[] {
  if (scope === "all" || item.mediaKind === "image") return [...item.entries];
  let keptVideo = false;
  return item.entries.filter((entry) => {
    if (!isVideoEntry(entry)) return true;
    if (keptVideo) return false;
    keptVideo = true;
    return true;
  });
}

function buildScopePlan(
  scope: "all" | "original",
): { entries: DownloadPlanEntry[]; skippedAudio: number } {
  const plan: DownloadPlanEntry[] = [];
  let skippedAudio = 0;
  const liveMode = state.settings.livePhotoExport;
  for (const item of state.results) {
    const selected = selectEntries(item, scope);
    const paired = new Set<number>();
    for (let index = 0; index < selected.length; index += 1) {
      if (paired.has(index)) continue;
      const entry = selected[index];
      if (isAudioEntry(entry)) {
        skippedAudio += 1;
        continue;
      }
      let liveUrl: string | null = null;
      if (liveMode && !isVideoEntry(entry)) {
        const next = selected[index + 1];
        if (next && !paired.has(index + 1) && isLiveEntry(next)) {
          paired.add(index + 1);
          liveUrl = next.url;
        }
      }
      plan.push({
        url: entry.url,
        title: item.title || "",
        extension: entry.extension,
        liveUrl,
        sizeBytes: entry.sizeBytes || 0,
      });
    }
  }
  return { entries: plan, skippedAudio };
}

function buildSinglePlan(
  title: string,
  entries: MediaItem_Serialize[],
): DownloadPlanEntry[] {
  const plan: DownloadPlanEntry[] = [];
  const paired = new Set<number>();
  const liveMode = state.settings.livePhotoExport;
  for (let index = 0; index < entries.length; index += 1) {
    if (paired.has(index)) continue;
    const entry = entries[index];
    let liveUrl: string | null = null;
    if (liveMode && !isVideoEntry(entry)) {
      const next = entries[index + 1];
      if (next && !paired.has(index + 1) && isLiveEntry(next)) {
        paired.add(index + 1);
        liveUrl = next.url;
      }
    }
    plan.push({
      url: entry.url,
      title,
      extension: entry.extension,
      liveUrl,
      sizeBytes: entry.sizeBytes || 0,
    });
  }
  return plan;
}

export async function requestDownload(): Promise<void> {
  if (!state.results.length) {
    showToast("没有可下载的视频链接", "error");
    return;
  }
  if (downloading) {
    showToast("当前正在下载中，请等待完成", "error");
    return;
  }
  if (state.results.some(hasMultipleVideoQualities)) {
    openDialog("download-selection");
    return;
  }
  await beginScopeDownload("all");
}

export async function beginScopeDownload(scope: "all" | "original"): Promise<void> {
  closeDialog();
  const { entries, skippedAudio } = buildScopePlan(scope);
  await startDownloadPlan(entries, skippedAudio);
}

export async function confirmSingleDownload(
  title: string,
  entries: MediaItem_Serialize[],
): Promise<void> {
  if (downloading) {
    showToast("当前正在下载中，请等待完成", "error");
    return;
  }
  await startDownloadPlan(buildSinglePlan(title, entries), 0);
}

async function startDownloadPlan(
  entries: DownloadPlanEntry[],
  skippedAudio: number,
): Promise<void> {
  if (!entries.length) {
    showToast(
      skippedAudio > 0
        ? `已跳过 ${skippedAudio} 个 MP3 文件，没有可保存到相册的资源`
        : "没有可保存到相册的资源",
    );
    return;
  }
  const totalSize = entries.reduce((sum, entry) => sum + entry.sizeBytes, 0);
  state.download = {
    total: entries.length,
    current: 0,
    completed: 0,
    currentProgress: 0,
    overall: 0,
    downloaded: 0,
    totalSize,
    currentSize: 0,
    currentTotal: entries[0].sizeBytes,
    skippedAudio,
    timedOut: 0,
    errors: [],
    success: 0,
    cancelled: false,
  };
  state.dialog = "download-progress";
  downloading = true;

  for (let index = 0; index < entries.length; index += 1) {
    const progress = state.download;
    if (!progress || progress.cancelled) break;
    const entry = entries[index];
    progress.current = index;
    progress.currentTotal = entry.sizeBytes;
    progress.currentSize = 0;
    progress.currentProgress = 0;
    progress.overall = progress.completed / progress.total;
    const completedBytes = entries
      .slice(0, index)
      .reduce((sum, item) => sum + item.sizeBytes, 0);
    let tickBusy = false;
    const ticker = window.setInterval(async () => {
      if (tickBusy || !state.download || state.download.cancelled) return;
      tickBusy = true;
      try {
        const measured = await unwrap(commands.downloadProgress());
        if (!state.download || state.download.cancelled) return;
        state.download.currentSize = measured.downloaded || 0;
        state.download.currentTotal =
          measured.total || state.download.currentTotal;
        state.download.currentProgress =
          state.download.currentTotal > 0
            ? Math.min(
                1,
                state.download.currentSize / state.download.currentTotal,
              )
            : 0;
        state.download.downloaded =
          completedBytes + state.download.currentSize;
        state.download.overall =
          (state.download.completed + state.download.currentProgress) /
          state.download.total;
      } catch {
        // A finished command can race the final poll; the completion path wins.
      } finally {
        tickBusy = false;
      }
    }, 120);
    try {
      const saved = await unwrap(
        commands.downloadMedia(entry.url, entry.extension, entry.liveUrl),
      );
      window.clearInterval(ticker);
      if (!state.download || state.download.cancelled) break;
      if (!saved) {
        state.download.errors.push("保存到相册失败");
      } else {
        state.download.success += 1;
      }
      state.download.downloaded =
        entries.slice(0, index + 1).reduce((sum, item) => sum + item.sizeBytes, 0);
      state.download.currentSize = entry.sizeBytes;
    } catch (error) {
      window.clearInterval(ticker);
      if (!state.download || state.download.cancelled) break;
      const message = String(error);
      state.download.errors.push(message);
      if (message.includes("超时")) state.download.timedOut += 1;
    }
    state.download.completed += 1;
    state.download.currentProgress = 1;
    state.download.overall = state.download.completed / state.download.total;
  }

  const finished = state.download;
  downloading = false;
  if (!finished) return;
  if (finished.cancelled) {
    state.download = null;
    state.dialog = null;
    showToast("下载已取消");
    return;
  }
  const failed = Math.min(
    finished.errors.length,
    finished.total - finished.success,
  );
  state.downloadSaved = {
    count: finished.success,
    failed,
    skipped: finished.skippedAudio,
    timedOut: finished.timedOut,
  };
  state.download = null;
  if (finished.success > 0) {
    state.dialog = "download-saved";
  } else if (finished.timedOut > 0 && finished.timedOut === finished.errors.length) {
    state.dialog = null;
    showToast(
      `${finished.timedOut} 个链接超过 10 秒无响应，已跳过`,
      "error",
    );
  } else if (finished.errors[0]) {
    const skipped =
      finished.skippedAudio > 0
        ? `（已跳过 ${finished.skippedAudio} 个 MP3 文件）`
        : "";
    state.dialog = null;
    showToast(`下载失败: ${finished.errors[0]}${skipped}`, "error");
  } else {
    state.dialog = null;
    showToast("下载失败: 0/" + finished.total + " 个视频", "error");
  }
}

export function cancelDownload(): void {
  if (state.download) state.download.cancelled = true;
  dialogCloseGuardUntil = performance.now() + 350;
  state.download = null;
  state.dialog = null;
  showToast("下载已取消");
}

export function stepExperienceItem(delta: number): void {
  if (!state.results.length) return;
  const next = Math.max(
    0,
    Math.min(state.results.length - 1, state.expItem + delta),
  );
  if (next === state.expItem) return;
  state.expItem = next;
  state.expSlide = 0;
  state.expSlideOffset = 0;
  state.expQualityMenu = false;
}

export function stepExperienceSlide(delta: number): void {
  const item = currentItem.value;
  if (!item) return;
  const count = slidesForItem(item).length;
  if (!count) return;
  state.expSlide = Math.max(0, Math.min(count - 1, state.expSlide + delta));
  state.expSlideOffset = 0;
  state.expQualityMenu = false;
}

export function selectQuality(entryIndex: number): void {
  state.qualityByItem[state.expItem] = entryIndex;
  state.expQualityMenu = false;
}

export function qualityIndexFor(slide: SlideSpec): number {
  const selectedEntry = state.qualityByItem[state.expItem];
  const index = slide.qualityIndices.indexOf(selectedEntry);
  return Math.max(0, index);
}

export function selectedExperienceEntry(slide: SlideSpec): MediaItem_Serialize | null {
  return selectedEntryForSlide(currentItem.value, slide);
}

export async function copyExperienceSlide(
  slide: SlideSpec,
  item: ExtractedResult_Serialize,
): Promise<void> {
  const base = slideBaseEntry(item, slide);
  const selected = selectedExperienceEntry(slide);
  const live =
    slide.kind === "live" && slide.liveIndex !== null
      ? item.entries[slide.liveIndex]
      : null;
  if (slide.kind === "live" && base) {
    await writeClipboard(live ? `${base.url}\n${live.url}` : base.url);
    showToast("已复制图片与视频链接", "success");
    return;
  }
  const entry = slide.kind === "cover" ? selected : selected || base;
  if (!entry) return;
  await writeClipboard(entry.url);
  showToast(slide.kind === "cover" ? "已复制封面链接" : "已复制链接", "success");
}

export async function copyExperienceTitle(): Promise<void> {
  const title = currentItem.value?.title || "";
  if (!title) return;
  await writeClipboard(title);
  showToast("已复制标题", "success");
}

export async function downloadExperienceSlide(
  slide: SlideSpec,
  item: ExtractedResult_Serialize,
): Promise<void> {
  const base = slideBaseEntry(item, slide);
  const selected = selectedExperienceEntry(slide);
  if (!base || !selected) return;
  if (slide.kind === "live") {
    const entries = slide.liveIndex === null
      ? [base]
      : [base, item.entries[slide.liveIndex]].filter(
          (entry): entry is MediaItem_Serialize => Boolean(entry),
        );
    await confirmSingleDownload(item.title || "", entries);
    return;
  }
  await confirmSingleDownload(item.title || "", [selected]);
}

async function refreshInsets(): Promise<void> {
  try {
    state.insets = await unwrap(commands.windowInsets());
    const left = state.insets.left || 0;
    const top = state.insets.top || 0;
    const right = state.insets.right || 0;
    const bottom = state.insets.bottom || 0;
    document.documentElement.style.setProperty(
      "--safe-left",
      `${Math.max(0, left)}px`,
    );
    document.documentElement.style.setProperty(
      "--safe-top",
      `${Math.max(0, top)}px`,
    );
    document.documentElement.style.setProperty(
      "--safe-right",
      `${Math.max(0, right)}px`,
    );
    document.documentElement.style.setProperty(
      "--safe-bottom",
      `${Math.max(0, bottom)}px`,
    );
  } catch {
    // CSS env(safe-area-inset-*) remains the fallback.
  }
}

function updateViewport(): void {
  state.viewportWidth = window.innerWidth;
  state.viewportHeight = window.innerHeight;
  state.visualHeight = window.visualViewport?.height ?? window.innerHeight;
  state.visualOffset = window.visualViewport?.offsetTop ?? 0;
  document.documentElement.style.setProperty(
    "--visual-height",
    `${Math.max(1, state.visualHeight)}px`,
  );
  document.documentElement.style.setProperty(
    "--visual-offset",
    `${Math.max(0, state.visualOffset)}px`,
  );
}

async function syncSystemBars(): Promise<void> {
  try {
    await unwrap(commands.setSystemBars(isDark.value));
  } catch {
    // The WebView theme still applies if native chrome is unavailable.
  }
}

export function initialize(): Promise<void> {
  if (initPromise) return initPromise;
  initialized = true;
  installTouchHover();
  document.body.dataset.dark = isDark.value ? "1" : "0";
  updateViewport();
  void refreshInsets();
  window.setTimeout(() => void refreshInsets(), 120);
  window.setTimeout(() => void refreshInsets(), 500);
  window.setTimeout(() => void refreshInsets(), 1200);
  void syncSystemBars();

  watch(isDark, (dark) => {
    document.body.dataset.dark = dark ? "1" : "0";
    void syncSystemBars();
  });

  systemTheme.addEventListener("change", (event) => {
    state.systemDark = event.matches;
  });

  window.addEventListener("resize", () => {
    updateViewport();
    void refreshInsets();
  });
  window.visualViewport?.addEventListener("resize", updateViewport);
  window.visualViewport?.addEventListener("scroll", updateViewport);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      state.systemDark = systemTheme.matches;
      updateViewport();
      void refreshInsets();
      void syncSystemBars();
    }
  });

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      navigateBack();
    }
  });

  watch(
    () => state.dialog,
    (dialog) => {
      window.clearInterval(insetTimer);
      if (dialog === "cookie") {
        insetTimer = window.setInterval(() => {
          void refreshInsets();
        }, 120);
      }
    },
  );

  watch(
    () => state.homeInputFocused,
    (focused) => {
      window.clearInterval(homeInsetTimer);
      if (!focused) return;
      homeInsetTimer = window.setInterval(() => {
        void refreshInsets();
      }, 120);
    },
  );

  initPromise = (async () => {
    try {
      state.settings = await unwrap(commands.loadSettings());
    } catch {
      state.settings = { ...defaultSettings };
    }
    try {
      state.version = await commands.appVersion();
    } catch {
      state.version = "1.0.6";
    }
    await refreshCacheInfo();
    void refreshInsets();
    void syncSystemBars();
  })();

  (window as unknown as { __REELS_BACK__: () => void }).__REELS_BACK__ =
    navigateBack;
  (
    window as unknown as { __REELS_SYNC_CHROME__: () => void }
  ).__REELS_SYNC_CHROME__ = () => {
    state.systemDark = systemTheme.matches;
    updateViewport();
    void refreshInsets();
    void syncSystemBars();
  };
  return initPromise;
}

function installTouchHover(): void {
  let clearTimer = 0;
  const clear = () => {
    document
      .querySelectorAll(".touch-hover")
      .forEach((element) => element.classList.remove("touch-hover"));
  };
  const scheduleClear = () => {
    window.clearTimeout(clearTimer);
    clearTimer = window.setTimeout(clear, 150);
  };
  document.addEventListener(
    "pointerdown",
    (event) => {
      window.clearTimeout(clearTimer);
      clear();
      const target = event.target;
      if (!(target instanceof Element)) return;
      const interactive = target.closest(
        "button, .settings-item, .result-entry, .result-view-toggle, .experience-item-nav, .quality-item, .carousel-arrow",
      );
      if (
        interactive &&
        !interactive.matches(":disabled") &&
        !interactive.classList.contains("disabled")
      ) {
        interactive.classList.add("touch-hover");
      }
    },
    true,
  );
  document.addEventListener("pointerup", scheduleClear, true);
  document.addEventListener("pointercancel", scheduleClear, true);
  document.addEventListener("scroll", scheduleClear, true);
  document.addEventListener(
    "click",
    (event) => {
      if (performance.now() >= dialogCloseGuardUntil) return;
      event.preventDefault();
      event.stopImmediatePropagation();
    },
    true,
  );
}
