<script setup lang="ts">
import { computed } from "vue";
import { ICONS } from "../icons";

const props = withDefaults(
  defineProps<{
    name?: string;
    asset?: string;
    size?: number;
    color?: string;
    mono?: boolean;
  }>(),
  {
    name: "",
    asset: "",
    size: 24,
    color: "#3f8dff",
    mono: false,
  },
);

const svg = computed(() => ICONS[props.name] || "");
const hasGradient = computed(() => svg.value.includes("url(#"));
const maskImage = computed(() => {
  const source = svg.value;
  return source
    ? `url("data:image/svg+xml,${encodeURIComponent(source)}")`
    : "none";
});
</script>

<template>
  <img
    v-if="asset"
    class="icon-glyph"
    :src="asset"
    :style="{ width: `${size}px`, height: `${size}px` }"
    alt=""
    draggable="false"
  />
  <span
    v-else-if="mono"
    class="icon-glyph icon-mask"
    :style="{
      width: `${size}px`,
      height: `${size}px`,
      backgroundColor: color,
      WebkitMaskImage: maskImage,
      maskImage,
    }"
  />
  <span
    v-else-if="hasGradient"
    class="icon-glyph icon-mask gradient-icon"
    :style="{
      width: `${size}px`,
      height: `${size}px`,
      WebkitMaskImage: maskImage,
      maskImage,
    }"
  />
  <span
    v-else
    class="icon-glyph inline-svg"
    :style="{ width: `${size}px`, height: `${size}px` }"
    v-html="svg"
  />
</template>
