<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import IconGlyph from "./IconGlyph.vue";
import {
  copyExperienceSlide,
  copyExperienceTitle,
  currentItem,
  downloadExperienceSlide,
  ensureThumbnail,
  formatDuration,
  formatSize,
  isAudioEntry,
  isLiveEntry,
  isVideoEntry,
  previewUrlFor,
  qualityIndexFor,
  retryThumbnails,
  selectedExperienceEntry,
  slideBaseEntry,
  slidesForItem,
  state,
  stepExperienceItem,
  type SlideSpec,
} from "../store";

const previewArea = ref<HTMLElement | null>(null);
const areaWidth = ref(1);
const areaHeight = ref(1);
let resizeObserver: ResizeObserver | null = null;
let resizeFrame = 0;

const slides = computed(() => slidesForItem(currentItem.value));
const safeSlideIndex = computed(() =>
  Math.max(0, Math.min(state.expSlide, slides.value.length - 1)),
);
const currentSlide = computed<SlideSpec | null>(
  () => slides.value[safeSlideIndex.value] ?? null,
);
const baseEntry = computed(() => {
  if (!currentItem.value || !currentSlide.value) return null;
  return slideBaseEntry(currentItem.value, currentSlide.value);
});
const selectedEntry = computed(() => {
  if (!currentSlide.value) return null;
  return selectedExperienceEntry(currentSlide.value);
});
const qualityPosition = computed(() => {
  if (!currentSlide.value) return 0;
  return qualityIndexFor(currentSlide.value);
});
const qualityEntries = computed(() => {
  if (!currentItem.value || !currentSlide.value) return [];
  return currentSlide.value.qualityIndices
    .map((index) => currentItem.value?.entries[index])
    .filter((entry) => Boolean(entry));
});

const isVideo = computed(() => currentSlide.value?.kind === "video");
const isLive = computed(() => currentSlide.value?.kind === "live");
const isAudio = computed(() => currentSlide.value?.kind === "audio");
const isCover = computed(() => currentSlide.value?.kind === "cover");

const primaryPreviewUrl = computed(() => {
  if (!currentItem.value || !currentSlide.value) return null;
  return previewUrlFor(currentItem.value, currentSlide.value);
});
const fallbackPreviewUrl = computed(() => {
  if (!baseEntry.value) return null;
  if (isCover.value) return baseEntry.value.url;
  if (currentSlide.value?.kind === "image" || isLive.value) {
    return baseEntry.value.url;
  }
  return null;
});
const activePreviewUrl = computed(() => {
  const primary = primaryPreviewUrl.value;
  const status = primary ? state.thumbnails[primary]?.status : undefined;
  if (status === "failed" && fallbackPreviewUrl.value) {
    return fallbackPreviewUrl.value;
  }
  return primary;
});
const previewState = computed(() =>
  activePreviewUrl.value ? state.thumbnails[activePreviewUrl.value] : undefined,
);
const previewData = computed(() =>
  previewState.value?.status === "ready" ? previewState.value.data : "",
);
const previewFailed = computed(() => previewState.value?.status === "failed");

watch(
  () => [primaryPreviewUrl.value, fallbackPreviewUrl.value],
  () => {
    ensureThumbnail(primaryPreviewUrl.value);
    ensureThumbnail(fallbackPreviewUrl.value);
  },
  { immediate: true },
);

const cardRatio = computed(() => {
  const selected = selectedEntry.value;
  const base = baseEntry.value;
  if (!selected) return null;
  if (isVideo.value) {
    if (selected.thumbnailWidth && selected.thumbnailHeight) {
      return selected.thumbnailWidth / selected.thumbnailHeight;
    }
    if (selected.width && selected.height) return selected.width / selected.height;
  }
  const entry = base || selected;
  if (entry.width && entry.height) return entry.width / entry.height;
  return null;
});

const cardSize = computed(() => {
  if (isAudio.value) {
    return {
      width: Math.min(areaWidth.value, 520),
      height: Math.min(areaHeight.value, 320),
    };
  }
  const ratio = cardRatio.value;
  if (!ratio || !Number.isFinite(ratio) || ratio <= 0) {
    return { width: areaWidth.value, height: areaHeight.value };
  }
  let width = areaWidth.value;
  let height = width / ratio;
  if (height > areaHeight.value) {
    height = areaHeight.value;
    width = height * ratio;
  }
  return { width: Math.max(1, width), height: Math.max(1, height) };
});

const neighborSlide = computed<SlideSpec | null>(() => {
  if (state.expSlideOffset < -1) return slides.value[safeSlideIndex.value + 1] || null;
  if (state.expSlideOffset > 1) return slides.value[safeSlideIndex.value - 1] || null;
  return null;
});
const neighborUrl = computed(() => {
  if (!currentItem.value || !neighborSlide.value) return null;
  return previewUrlFor(currentItem.value, neighborSlide.value);
});
const neighborData = computed(() => {
  const url = neighborUrl.value;
  return url && state.thumbnails[url]?.status === "ready"
    ? state.thumbnails[url].data
    : "";
});
watch(
  neighborUrl,
  (url) => ensureThumbnail(url),
  { immediate: true },
);

function resolutionText(entry: typeof selectedEntry.value): string | null {
  if (!entry) return null;
  if (entry.width && entry.height) return `${entry.width}×${entry.height}`;
  return null;
}

const rightMeta = computed(() => {
  const entry = selectedEntry.value;
  if (!entry) return "";
  const parts: string[] = [];
  const resolution = resolutionText(entry);
  if (resolution) parts.push(resolution);
  if (isVideo.value && entry.durationSecs) {
    parts.push(formatDuration(entry.durationSecs));
  }
  return parts.join(" · ");
});

const qualityLabel = computed(() => {
  const entry = selectedEntry.value;
  if (!entry) return "";
  return (
    entry.quality?.trim() ||
    `清晰度 ${qualityPosition.value + 1}`
  );
});

const qualityChoice = computed(
  () => isVideo.value && qualityEntries.value.length > 1,
);
const canPrevSlide = computed(() => safeSlideIndex.value > 0);
const canNextSlide = computed(
  () => safeSlideIndex.value + 1 < slides.value.length,
);
const canPrevItem = computed(() => state.expItem > 0);
const canNextItem = computed(() => state.expItem + 1 < state.results.length);

const downloadLabel = computed(() => {
  if (isVideo.value) return "下载视频";
  if (isAudio.value) return "下载音频";
  return "下载图片";
});
const downloadEnabled = computed(() => !isAudio.value);

function measure(): void {
  const element = previewArea.value;
  if (!element) return;
  const style = getComputedStyle(element);
  const horizontalPadding =
    Number.parseFloat(style.paddingLeft || "0") +
    Number.parseFloat(style.paddingRight || "0");
  const verticalPadding =
    Number.parseFloat(style.paddingTop || "0") +
    Number.parseFloat(style.paddingBottom || "0");
  areaWidth.value = Math.max(1, element.clientWidth - horizontalPadding);
  areaHeight.value = Math.max(1, element.clientHeight - verticalPadding);
}

function scheduleMeasure(): void {
  cancelAnimationFrame(resizeFrame);
  resizeFrame = requestAnimationFrame(measure);
}

onMounted(() => {
  resizeObserver = new ResizeObserver(scheduleMeasure);
  if (previewArea.value) resizeObserver.observe(previewArea.value);
  scheduleMeasure();
  window.addEventListener("pointerdown", onGlobalPointerDown, true);
});
onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  cancelAnimationFrame(resizeFrame);
  window.removeEventListener("pointerdown", onGlobalPointerDown, true);
});
watch(
  () => [state.expItem, state.expTab, state.expSlide],
  () => nextTick(scheduleMeasure),
);

let dragStartX = 0;
let pointerId = -1;
let dragActive = false;
let slideRaf = 0;
let slideAnimSeq = 0;

function onPointerDown(event: PointerEvent): void {
  if ((event.target as HTMLElement).closest("button")) return;
  if (!canPrevSlide.value && !canNextSlide.value) return;
  dragActive = true;
  dragStartX = event.clientX;
  pointerId = event.pointerId;
  slideAnimSeq += 1;
  cancelAnimationFrame(slideRaf);
  state.expSlideAnimating = false;
  state.expSlideOffset = 0;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

function onGlobalPointerDown(event: PointerEvent): void {
  if (!state.expQualityMenu) return;
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (target.closest(".quality-menu") || target.closest(".quality-toggle")) {
    return;
  }
  state.expQualityMenu = false;
}

function onPointerMove(event: PointerEvent): void {
  if (!dragActive || event.pointerId !== pointerId) return;
  let dx = event.clientX - dragStartX;
  if ((dx < 0 && !canNextSlide.value) || (dx > 0 && !canPrevSlide.value)) {
    dx *= 0.3;
  }
  const limit = cardSize.value.width;
  state.expSlideOffset = Math.max(-limit, Math.min(limit, dx));
}

function finishSlideDrag(event?: PointerEvent): void {
  if (!dragActive) return;
  dragActive = false;
  if (event) {
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      // The pointer may already have been released by the WebView.
    }
  }
  const width = Math.max(1, cardSize.value.width);
  const threshold = width * 0.18;
  const offset = state.expSlideOffset;
  const delta =
    offset <= -threshold && canNextSlide.value
      ? 1
      : offset >= threshold && canPrevSlide.value
        ? -1
        : 0;
  animateTo(delta, width);
}

function animateTo(delta: number, width = Math.max(1, cardSize.value.width)): void {
  const target = delta === 1 ? -width : delta === -1 ? width : 0;
  const from = state.expSlideOffset;
  if (delta === 0 && Math.abs(from) < 0.5) {
    state.expSlideOffset = 0;
    state.expSlideAnimating = false;
    return;
  }
  const seq = ++slideAnimSeq;
  cancelAnimationFrame(slideRaf);
  state.expSlideAnimating = true;
  const startedAt = performance.now();
  const step = (now: number) => {
    if (seq !== slideAnimSeq) return;
    const linear = Math.min(1, (now - startedAt) / 200);
    const eased = 1 - Math.pow(1 - linear, 3);
    state.expSlideOffset = from + (target - from) * eased;
    if (linear >= 1) {
      state.expSlideOffset = 0;
      state.expSlideAnimating = false;
      if (delta !== 0) {
        state.expSlide = Math.max(
          0,
          Math.min(slides.value.length - 1, state.expSlide + delta),
        );
        state.expQualityMenu = false;
      }
      return;
    }
    slideRaf = requestAnimationFrame(step);
  };
  slideRaf = requestAnimationFrame(step);
}

function onPointerUp(event: PointerEvent): void {
  if (event.pointerId === pointerId) finishSlideDrag(event);
}

function onPointerCancel(event: PointerEvent): void {
  if (event.pointerId === pointerId) finishSlideDrag(event);
}

function stepBy(delta: number): void {
  if (state.expSlideAnimating) return;
  animateTo(delta);
}

function slideEntriesForDownload(): NonNullable<typeof baseEntry.value>[] {
  const base = baseEntry.value;
  const selected = selectedEntry.value;
  if (!base || !selected) return [];
  if (isLive.value && currentItem.value && currentSlide.value?.liveIndex !== null) {
    const live = currentItem.value.entries[currentSlide.value?.liveIndex ?? -1];
    return live ? [base, live] : [base];
  }
  return [selected];
}

function downloadCurrent(): void {
  if (!currentSlide.value || !currentItem.value || !downloadEnabled.value) return;
  void downloadExperienceSlide(currentSlide.value, currentItem.value);
}

function copyCurrent(): void {
  if (!currentSlide.value || !currentItem.value) return;
  void copyExperienceSlide(currentSlide.value, currentItem.value);
}

function selectQuality(entryIndex: number): void {
  state.qualityByItem[state.expItem] = entryIndex;
  state.expQualityMenu = false;
}
</script>

<template>
  <div class="experience-page">
    <div class="experience-tabbar">
      <button
        class="experience-item-nav"
        :class="{ disabled: !canPrevItem }"
        type="button"
        :disabled="!canPrevItem"
        @click="stepExperienceItem(-1)"
      >
        上一个
      </button>
      <div class="experience-tabs">
        <button
          class="experience-tab"
          :class="{ active: state.expTab === 'all' }"
          type="button"
          @click="state.expTab = 'all'; state.expQualityMenu = false"
        >
          全部
        </button>
        <button
          class="experience-tab"
          :class="{ active: state.expTab === 'title' }"
          type="button"
          @click="state.expTab = 'title'; state.expQualityMenu = false"
        >
          标题
        </button>
      </div>
      <button
        class="experience-item-nav"
        :class="{ disabled: !canNextItem }"
        type="button"
        :disabled="!canNextItem"
        @click="stepExperienceItem(1)"
      >
        下一个
      </button>
    </div>

    <template v-if="state.expTab === 'title'">
      <div class="experience-title-tab">
        <div class="experience-title-scroll">
          <div class="experience-title-box">{{ currentItem?.title || "" }}</div>
        </div>
        <button
          class="action-button copy-action experience-title-copy"
          type="button"
          :disabled="!currentItem?.title"
          @click="copyExperienceTitle"
        >
          <span class="button-content">
            <IconGlyph
              name="COPY"
              :size="16"
              :mono="state.settings.themeMode === 'dark' || (state.settings.themeMode === 'system' && state.systemDark)"
              :color="state.settings.themeMode === 'dark' || (state.settings.themeMode === 'system' && state.systemDark) ? 'var(--text)' : 'var(--primary)'"
            />
            <span>复制</span>
          </span>
        </button>
      </div>
    </template>

    <template v-else>
      <div v-if="currentSlide" ref="previewArea" class="experience-preview-area">
        <div
          class="preview-card"
          :style="{ width: `${cardSize.width}px`, height: `${cardSize.height}px` }"
          @pointerdown="onPointerDown"
          @pointermove="onPointerMove"
          @pointerup="onPointerUp"
          @pointercancel="onPointerCancel"
        >
          <div
            class="experience-slides"
            :class="{ animating: state.expSlideAnimating }"
          >
            <div
              class="experience-slide"
              :style="{ transform: `translate3d(${state.expSlideOffset}px,0,0)` }"
            >
              <div v-if="isAudio" class="preview-audio">音频文件</div>
              <img
                v-else-if="previewData"
                class="preview-image"
                :src="previewData"
                alt=""
                draggable="false"
              />
              <div v-else-if="previewFailed" class="preview-state preview-failed">
                <button
                  class="preview-retry"
                  type="button"
                  @click="retryThumbnails([primaryPreviewUrl, fallbackPreviewUrl])"
                >
                  <IconGlyph
                    name="RETRY"
                    :size="22"
                    mono
                    color="var(--text-muted)"
                  />
                </button>
                <span>预览加载失败</span>
              </div>
              <div v-else class="preview-state">预览加载中…</div>
            </div>

            <div
              v-if="neighborData"
              class="experience-neighbor-slide"
              :style="{
                left:
                  state.expSlideOffset < 0
                    ? `${state.expSlideOffset + cardSize.width}px`
                    : `${state.expSlideOffset - cardSize.width}px`,
              }"
            >
              <img class="preview-image" :src="neighborData" alt="" draggable="false" />
            </div>
          </div>

          <div v-if="isLive" class="live-badge">
            <IconGlyph
              asset="./assets/icons/live-photo-white.svg"
              :size="16"
              class="live-badge-icon"
            />
            <span>LIVE</span>
          </div>

          <div v-if="isVideo" class="video-play-overlay">
            <span class="video-play-circle">
              <IconGlyph name="PLAY_OVERLAY" :size="30" />
            </span>
          </div>

          <div v-if="selectedEntry?.sizeBytes" class="preview-pill preview-size-pill">
            {{ formatSize(selectedEntry.sizeBytes) }}
          </div>
          <div v-if="rightMeta" class="preview-pill preview-meta-pill">
            {{ rightMeta }}
          </div>

          <div v-if="qualityChoice" class="quality-toggle-wrap">
            <button
              class="quality-toggle"
              type="button"
              @click.stop="state.expQualityMenu = !state.expQualityMenu"
            >
              <span>{{ qualityLabel }}</span>
              <IconGlyph name="CHEVRON_DOWN" :size="12" mono color="#ffffff" />
            </button>
          </div>

          <div v-if="qualityChoice && state.expQualityMenu" class="quality-menu-wrap">
            <div class="quality-menu">
              <button
                v-for="(entry, position) in qualityEntries"
                :key="position"
                class="quality-item"
                :class="{ active: position === qualityPosition }"
                type="button"
                @click.stop="selectQuality(currentSlide?.qualityIndices[position] ?? 0)"
              >
                <span>
                  {{ entry?.quality?.trim() || `清晰度 ${position + 1}` }}
                </span>
                <IconGlyph
                  v-if="position === qualityPosition"
                  name="CHECK"
                  :size="12"
                  mono
                  color="var(--primary)"
                />
              </button>
            </div>
          </div>

          <div v-if="isCover" class="cover-tag">封面</div>

          <button
            v-if="canPrevSlide"
            class="carousel-arrow carousel-arrow-left"
            type="button"
            aria-label="上一张"
            @pointerdown.stop
            @click.stop="stepBy(-1)"
          >
            <span class="carousel-arrow-circle">
              <IconGlyph
                name="CHEVRON_LEFT"
                :size="20"
                mono
                color="#ffffff"
              />
            </span>
          </button>
          <button
            v-if="canNextSlide"
            class="carousel-arrow carousel-arrow-right"
            type="button"
            aria-label="下一张"
            @pointerdown.stop
            @click.stop="stepBy(1)"
          >
            <span class="carousel-arrow-circle">
              <IconGlyph
                name="CHEVRON_RIGHT"
                :size="20"
                mono
                color="#ffffff"
              />
            </span>
          </button>
        </div>
      </div>

      <div v-else class="experience-empty">暂无可预览的内容</div>

      <div class="experience-actions">
        <button class="action-button copy-action" type="button" @click="copyCurrent">
          <span class="button-content">
            <IconGlyph
              name="COPY"
              :size="16"
              :mono="state.settings.themeMode === 'dark' || (state.settings.themeMode === 'system' && state.systemDark)"
              :color="state.settings.themeMode === 'dark' || (state.settings.themeMode === 'system' && state.systemDark) ? 'var(--text)' : 'var(--primary)'"
            />
            <span>复制链接</span>
          </span>
        </button>
        <button
          class="action-button primary-action"
          type="button"
          :disabled="!downloadEnabled"
          @click="downloadCurrent"
        >
          <span class="button-content">
            <IconGlyph name="DOWNLOAD" :size="16" mono color="#ffffff" />
            <span>{{ downloadLabel }}</span>
          </span>
        </button>
      </div>
    </template>
  </div>
</template>
