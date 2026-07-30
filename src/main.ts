console.log("========== MAIN TS LOADED ==========");

import { getCurrentWindow } from "@tauri-apps/api/window";

async function boot() {
  console.log("BOOT START");

  // 检测是否为图片预览窗口
  // 优先检查 initialization_script 设置的标志（不依赖元数据注入）
  let isViewer = (window as any).__SIDEBAR_MEMO_VIEWER__ === true;
  if (!isViewer) {
    try {
      const win = getCurrentWindow();
      console.log("WINDOW LABEL:", win.label);
      isViewer = win.label === "image-viewer";
    } catch (e) {
      console.warn("getCurrentWindow failed:", e);
    }
  }

  // 图片预览窗口：只挂轻量 viewer，不加载主应用
  if (isViewer) {
    try {
      console.log(">>> Entering image-viewer branch");
      document.documentElement.style.background = "#111";
      document.body.style.background = "#111";
      document.body.style.margin = "0";
      document.body.style.overflow = "hidden";
      document.body.style.width = "100%";
      document.body.style.height = "100%";
      const app = document.getElementById("app");
      if (app) {
        app.style.width = "100%";
        app.style.height = "100%";
        console.log(">>> Importing viewer-app...");
        const { mountViewer } = await import("./viewer-app");
        console.log(">>> Mounting viewer...");
        await mountViewer(app);
        console.log(">>> Viewer mounted");
      } else {
        console.error(">>> #app element not found");
      }
      return;
    } catch (e) {
      console.error("viewer boot error:", e);
    }
  }

  console.log(">>> Entering main Vue app branch");
  const { createApp } = await import("vue");
  const { default: App } = await import("./App.vue");
  await import("./assets/style.css");
  createApp(App).mount("#app");
}

boot();
