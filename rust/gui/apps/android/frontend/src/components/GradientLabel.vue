<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    kind: "paste" | "clear" | "parse";
    width?: number;
    height?: number;
    colored?: boolean;
    color?: string;
  }>(),
  {
    width: 27,
    height: 13,
    colored: true,
    color: "#3f8dff",
  },
);

const asset = computed(() => `./assets/gradient/${props.kind}.svg`);
const mask = computed(
  () => `url("${asset.value}") center / contain no-repeat`,
);
</script>

<template>
  <img
    v-if="colored"
    class="gradient-label"
    :src="asset"
    :style="{ width: `${width}px`, height: `${height}px` }"
    alt=""
    draggable="false"
  />
  <span
    v-else
    class="gradient-label gradient-label-mask"
    :style="{
      width: `${width}px`,
      height: `${height}px`,
      backgroundColor: color,
      WebkitMask: mask,
      mask,
    }"
  />
</template>
