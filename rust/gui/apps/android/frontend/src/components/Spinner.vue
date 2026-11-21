<script setup lang="ts">
withDefaults(defineProps<{ size?: number; color?: string }>(), {
  size: 36,
  color: "#3f8dff",
});
</script>

<template>
  <span
    class="spinner"
    :style="{ width: `${size}px`, height: `${size}px`, color }"
    aria-label="加载中"
  >
    <i v-for="index in 8" :key="index" :style="{ '--i': index - 1 }" />
  </span>
</template>
