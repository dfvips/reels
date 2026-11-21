<script setup lang="ts">
import type { PropType } from "vue";
import IconGlyph from "./IconGlyph.vue";
import {
  cycleTheme,
  navigateBack,
  openSettings,
  state,
  themeIcon,
  toggleResultView,
  type Route,
} from "../store";

defineProps({
  route: {
    type: String as PropType<Route>,
    required: true,
  },
  compact: {
    type: Boolean,
    default: false,
  },
});
</script>

<template>
  <section class="page-shell" :class="`route-${route}`">
    <header
      v-if="!(compact && route === 'home')"
      class="appbar"
      :class="{ 'appbar-home': route === 'home' }"
    >
      <button
        v-if="route !== 'home'"
        class="appbar-icon-button"
        type="button"
        aria-label="返回"
        @click="navigateBack"
      >
        <IconGlyph name="BACK" :size="24" />
      </button>
      <div v-else class="appbar-home-title">
        <img class="home-logo" src="/assets/logo.svg" alt="" draggable="false" />
        <img
          class="home-title"
          src="/assets/gradient/title.svg"
          alt="短视频解析"
          draggable="false"
        />
      </div>

      <div v-if="route !== 'home'" class="appbar-center-title">
        {{ route === "settings" ? "设置" : "解析结果" }}
      </div>

      <div v-if="route === 'home'" class="appbar-trailing">
        <button
          class="appbar-icon-button"
          type="button"
          aria-label="切换主题"
          @click="cycleTheme"
        >
          <IconGlyph :name="themeIcon" :size="24" />
        </button>
        <button
          class="appbar-icon-button"
          type="button"
          aria-label="设置"
          @click="openSettings"
        >
          <IconGlyph name="SETTINGS" :size="24" />
        </button>
      </div>
      <div v-else class="appbar-trailing-spacer" />

      <button
        v-if="route === 'results'"
        class="result-view-toggle"
        type="button"
        @click="toggleResultView"
      >
        <IconGlyph
          name="SWITCH"
          :size="14"
          mono
          color="var(--text-secondary)"
        />
        <span>{{ state.settings.resultView === "geek" ? "体验版" : "极客版" }}</span>
      </button>
    </header>
    <div class="page-body">
      <slot />
    </div>
  </section>
</template>
