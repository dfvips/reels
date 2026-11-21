import { createApp, nextTick } from "vue";
import App from "./App.vue";
import { commands } from "./bindings";
import { unwrap } from "./ipc";
import { initialize } from "./store";
import "./style.css";

declare global {
  interface Window {
    __REELS_BACK__?: () => void;
    __REELS_SYNC_CHROME__?: () => void;
  }
}

createApp(App).mount("#app");
void initialize().then(async () => {
  await nextTick();
  window.setTimeout(() => {
    void unwrap(commands.androidReady()).catch(() => undefined);
  }, 40);
});
