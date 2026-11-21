<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import GradientLabel from "./GradientLabel.vue";
import IconGlyph from "./IconGlyph.vue";
import PageShell from "./PageShell.vue";
import Spinner from "./Spinner.vue";
import {
  clearInput,
  compactHome,
  isDark,
  parseLinks,
  pasteFromClipboard,
  state,
} from "../store";

const textarea = ref<HTMLTextAreaElement | null>(null);
const INPUT_CHROME_HEIGHT = 24;
const inputHeight = ref(144);

const hasContent = computed(() => state.input.length > 0);
const inputLineCap = computed(() => {
  const maxHeight = Math.max(state.viewportHeight / 3, 44);
  return Math.max(1, Math.floor((maxHeight - INPUT_CHROME_HEIGHT) / 20));
});

function resizeInput(): void {
  nextTick(() => {
    const field = textarea.value;
    if (!field) return;
    if (compactHome.value) {
      const available = Math.max(
        38,
        state.viewportHeight -
          (state.insets.top || 0) -
          (state.insets.bottom || 0) -
          (state.insets.imeBottom || 0) -
          8,
      );
      inputHeight.value = available;
      field.style.height = `${available}px`;
      return;
    }
    const cap = inputLineCap.value;
    const baseLines = Math.min(6, cap);
    field.style.height = "1px";
    const measured = field.scrollHeight;
    const contentLines = Math.max(
      1,
      Math.ceil((measured - INPUT_CHROME_HEIGHT) / 20),
    );
    const lines = Math.max(baseLines, Math.min(cap, contentLines));
    inputHeight.value = lines * 20 + INPUT_CHROME_HEIGHT;
    field.style.height = `${inputHeight.value}px`;
  });
}

watch(() => state.input, resizeInput);
watch(inputLineCap, resizeInput);
watch(
  () => [
    compactHome.value,
    state.insets.imeBottom,
    state.insets.top,
    state.insets.bottom,
  ],
  resizeInput,
);
onMounted(resizeInput);
</script>

<template>
  <PageShell route="home" :compact="compactHome">
    <div class="home-scroll">
      <div class="home-content" :class="{ compact: compactHome }">
        <div class="home-input-card">
          <textarea
            ref="textarea"
            id="url-input"
            class="home-input"
            :style="{ height: `${inputHeight}px` }"
            placeholder="输入或粘贴短视频分享链接，支持多链接解析"
            v-model="state.input"
            @focus="state.homeInputFocused = true"
            @blur="state.homeInputFocused = false"
            spellcheck="false"
            autocomplete="off"
            autocapitalize="off"
          />

          <div v-if="!compactHome" class="home-actions">
            <button
              class="action-button home-button button-paste"
              type="button"
              @click="pasteFromClipboard"
            >
              <span class="button-content">
                <IconGlyph
                  name="PASTE"
                  :size="16"
                  :mono="isDark"
                  :color="isDark ? 'var(--text)' : 'var(--primary)'"
                />
                <span class="gradient-label-slot" style="width: 29px">
                  <GradientLabel
                    kind="paste"
                    :width="27"
                    :height="13"
                    :colored="!isDark"
                    :color="isDark ? 'var(--text)' : 'var(--primary)'"
                  />
                </span>
              </span>
            </button>

            <button
              class="action-button home-button button-clear"
              :class="{ 'is-enabled': hasContent }"
              type="button"
              :disabled="!hasContent"
              @pointerdown.prevent="hasContent && clearInput()"
            >
              <span class="button-content">
                <IconGlyph
                  name="REFRESH"
                  :size="20"
                  :mono="!hasContent"
                  :color="hasContent ? 'var(--primary)' : 'var(--button-disabled-fg)'"
                />
                <span class="gradient-label-slot" style="width: 28px">
                  <GradientLabel
                    kind="clear"
                    :width="26"
                    :height="13"
                    :colored="hasContent"
                    :color="hasContent ? 'var(--primary)' : 'var(--button-disabled-fg)'"
                  />
                </span>
              </span>
            </button>

            <button
              class="action-button home-button button-parse"
              :class="{ 'is-enabled': hasContent }"
              type="button"
              :disabled="!hasContent"
              @click="parseLinks"
            >
              <span class="button-content">
                <IconGlyph
                  name="SEND"
                  :size="16"
                  :mono="true"
                  :color="hasContent ? '#ffffff' : 'var(--button-disabled-fg)'"
                />
                <span class="gradient-label-slot" style="width: 29px">
                  <GradientLabel
                    kind="parse"
                    :width="27"
                    :height="13"
                    :colored="false"
                    :color="hasContent ? '#ffffff' : 'var(--button-disabled-fg)'"
                  />
                </span>
              </span>
            </button>
          </div>
        </div>

        <div v-if="state.loading" class="home-loading">
          <Spinner :size="36" color="var(--primary)" />
          <span>正在解析链接...</span>
        </div>
      </div>
    </div>
  </PageShell>
</template>
