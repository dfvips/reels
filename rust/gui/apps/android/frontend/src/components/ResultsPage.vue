<script setup lang="ts">
import ExperienceResults from "./ExperienceResults.vue";
import GeekResults from "./GeekResults.vue";
import PageShell from "./PageShell.vue";
import { state } from "../store";
</script>

<template>
  <PageShell route="results">
    <div v-if="!state.results.length" class="results-empty">没有解析结果</div>
    <GeekResults v-else-if="state.settings.resultView === 'geek'" />
    <ExperienceResults v-else />
  </PageShell>
</template>
