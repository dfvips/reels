<script setup lang="ts">
import { computed } from "vue";
import IconGlyph from "./IconGlyph.vue";
import {
  copyAll,
  copyEntry,
  entryMeta,
  isDark,
  requestDownload,
  state,
} from "../store";

const linkCount = computed(() =>
  state.results.reduce((sum, item) => sum + item.entries.length, 0),
);
</script>

<template>
  <div class="results-geek">
    <div class="results-scroll">
      <div
        v-for="(item, itemIndex) in state.results"
        :key="itemIndex"
        class="result-group"
      >
        <div class="result-heading">
          <div class="result-number">{{ itemIndex + 1 }}</div>
          <div class="result-heading-copy">
            <div class="result-title">
              <span class="result-title-text">{{ item.title }}</span>
              <span
                class="result-kind-badge"
                :class="{ image: item.mediaKind === 'image' }"
              >
                {{ item.mediaKind === "image" ? "图片" : "视频" }}
              </span>
            </div>
          </div>
        </div>

        <button
          v-for="(entry, entryIndex) in item.entries"
          :key="entryIndex"
          class="result-entry"
          type="button"
          @click="copyEntry(entry)"
        >
          <span class="result-url">{{ entry.url }}</span>
          <span class="result-meta-row">
            <span class="result-meta">{{ entryMeta(entry) }}</span>
            <span class="result-copy-hint">
              <span class="result-copy-idle">点击链接复制</span>
              <span class="result-copy-active">
                <IconGlyph
                  name="LINK"
                  :size="13"
                  mono
                  color="var(--primary)"
                />
                <span>复制链接</span>
              </span>
            </span>
          </span>
        </button>
      </div>
    </div>

    <div class="results-actions">
      <button class="action-button copy-action" type="button" @click="copyAll">
        <span class="button-content">
          <IconGlyph
            name="COPY"
            :size="16"
            :mono="isDark"
            :color="isDark ? 'var(--text)' : 'var(--primary)'"
          />
          <span>复制链接 ({{ linkCount }})</span>
        </span>
      </button>
      <button
        class="action-button primary-action"
        type="button"
        @click="requestDownload"
      >
        <span class="button-content">
          <IconGlyph name="DOWNLOAD" :size="16" mono color="#ffffff" />
          <span>下载资源</span>
        </span>
      </button>
    </div>
  </div>
</template>
