<script setup lang="ts">
import IconGlyph from "./IconGlyph.vue";
import PageShell from "./PageShell.vue";
import {
  formatSize,
  openClearCache,
  openDialog,
  state,
} from "../store";
</script>

<template>
  <PageShell route="settings">
    <div class="settings-scroll">
      <div class="settings-list">
        <button class="settings-item" type="button" @click="openDialog('live')">
          <IconGlyph
            asset="./assets/icons/live-photo.svg"
            :size="24"
            class="settings-icon"
          />
          <span class="settings-copy">
            <span class="settings-title">实况照片</span>
            <span class="settings-subtitle">
              {{
                state.settings.livePhotoExport
                  ? "保存到相册时封装为 Motion Photo"
                  : "保存到相册时保存为图片和视频"
              }}
            </span>
          </span>
        </button>
        <div class="settings-divider" />

        <button class="settings-item" type="button" @click="openDialog('cookie')">
          <IconGlyph name="LOCK" :size="24" class="settings-icon" />
          <span class="settings-copy">
            <span class="settings-title">Cookie 设置</span>
            <span class="settings-subtitle">
              仅用于小红书 ·
              {{
                state.settings.xhsCookie.trim()
                  ? "已设置（用于小红书解析）"
                  : "未设置"
              }}
            </span>
          </span>
        </button>
        <div class="settings-divider" />

        <button class="settings-item" type="button" @click="openDialog('help')">
          <IconGlyph name="HELP" :size="24" class="settings-icon" />
          <span class="settings-copy">
            <span class="settings-title">使用说明</span>
            <span class="settings-subtitle">查看支持的链接格式与使用方式</span>
          </span>
        </button>
        <div class="settings-divider" />

        <button class="settings-item" type="button" @click="openDialog('about')">
          <IconGlyph name="INFO" :size="24" class="settings-icon" />
          <span class="settings-copy">
            <span class="settings-title">关于</span>
            <span class="settings-subtitle">应用版本: {{ state.version }}</span>
          </span>
        </button>
        <div class="settings-divider" />

        <button class="settings-item" type="button" @click="openClearCache">
          <IconGlyph name="TRASH" :size="24" class="settings-icon" />
          <span class="settings-copy">
            <span class="settings-title">清除缓存</span>
            <span class="settings-subtitle">
              当前缓存: {{ formatSize(state.cacheInfo.size) }}
              <template v-if="state.cachePathCount > 0">
                · {{ state.cachePathCount }}个位置
              </template>
            </span>
          </span>
        </button>
      </div>
    </div>
  </PageShell>
</template>
