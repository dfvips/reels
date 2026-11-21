<script setup lang="ts">
import { computed } from "vue";
import DialogHost from "./components/DialogHost.vue";
import HomePage from "./components/HomePage.vue";
import ResultsPage from "./components/ResultsPage.vue";
import SettingsPage from "./components/SettingsPage.vue";
import { state, type Route } from "./store";

function pageComponent(route: Route) {
  if (route === "settings") return SettingsPage;
  if (route === "results") return ResultsPage;
  return HomePage;
}

const baseRoute = computed<Route>(() => {
  if (!state.transition) return state.route;
  return state.transition.kind === "push"
    ? state.transition.from
    : state.transition.to;
});

const movingRoute = computed<Route | null>(() => {
  if (!state.transition) return null;
  return state.transition.kind === "push"
    ? state.transition.to
    : state.transition.from;
});

const movingStyle = computed(() => {
  if (!state.transition) return {};
  return {
    transform: `translate3d(${state.transition.offset}px, 0, 0)`,
  };
});
</script>

<template>
  <div id="app">
    <main class="app-stage">
      <template v-if="state.transition">
        <div class="page-layer transition-base">
          <component :is="pageComponent(baseRoute)" />
        </div>
        <div
          class="page-layer transition-moving"
          :class="{ animate: state.transition.animate }"
          :style="movingStyle"
        >
          <component
            :is="pageComponent(movingRoute || state.route)"
            :key="movingRoute || state.route"
          />
        </div>
        <div class="transition-input-lock" />
      </template>
      <div v-else class="page-layer">
        <component :is="pageComponent(state.route)" :key="state.route" />
      </div>
    </main>

    <div v-if="state.toast" class="toast" role="status">
      {{ state.toast.message }}
    </div>

    <DialogHost v-if="state.dialog" />
  </div>
</template>
