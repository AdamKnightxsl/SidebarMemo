import { ref } from "vue";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

const updating = ref(false);
const updateAvailable = ref(false);
const updateVersion = ref("");
const downloadProgress = ref(0);
const lastError = ref("");

export function useUpdater() {
  async function checkForUpdates() {
    lastError.value = "";
    updateAvailable.value = false;
    try {
      console.log("[Updater] 开始检查更新...");
      const update = await check();
      console.log("[Updater] 检查结果:", update ? `发现新版本 ${update.version}` : "无更新");
      if (update) {
        updateAvailable.value = true;
        updateVersion.value = update.version || "";
        return update;
      }
    } catch (e: any) {
      lastError.value = e?.message || String(e);
      console.error("[Updater] 检查更新失败:", e);
    }
    return null;
  }

  async function installUpdate(update: any) {
    if (updating.value) return;
    updating.value = true;
    downloadProgress.value = 0;
    lastError.value = "";

    try {
      await update.downloadAndInstall((event: any) => {
        if (event.event === "Started") {
          downloadProgress.value = 0;
        } else if (event.event === "Progress") {
          downloadProgress.value = event.data.percent || 0;
        } else if (event.event === "Finished") {
          downloadProgress.value = 100;
        }
      });
      await relaunch();
    } catch (e: any) {
      lastError.value = e?.message || String(e);
      console.error("安装更新失败:", e);
      updating.value = false;
    }
  }

  return {
    updating,
    updateAvailable,
    updateVersion,
    downloadProgress,
    lastError,
    checkForUpdates,
    installUpdate,
  };
}
