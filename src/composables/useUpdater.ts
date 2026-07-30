import { ref } from "vue";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

const updating = ref(false);
const updateAvailable = ref(false);
const updateVersion = ref("");
const downloadProgress = ref(0);
const lastError = ref("");
let cachedUpdate: any = null;

export function useUpdater() {
  async function checkForUpdates() {
    lastError.value = "";
    updateAvailable.value = false;
    try {
      const update = await check();
      if (update) {
        cachedUpdate = update;
        updateAvailable.value = true;
        updateVersion.value = update.version || "";
        return update;
      }
    } catch (e: any) {
      const msg = e?.message || String(e);
      lastError.value = msg.includes("error sending request for url")
        ? "网络错误"
        : msg;
      console.error("检查更新失败:", e);
    }
    return null;
  }

  async function installUpdate(update?: any) {
    const target = update || cachedUpdate;
    if (!target || updating.value) return;
    updating.value = true;
    downloadProgress.value = 0;
    lastError.value = "";

    try {
      let totalBytes = 0;
      let downloaded = 0;
      await target.downloadAndInstall((event: any) => {
        if (event.event === "Started") {
          totalBytes = event.data?.contentLength || 0;
          downloaded = 0;
          downloadProgress.value = 0;
        } else if (event.event === "Progress") {
          downloaded += event.data?.chunkLength || 0;
          downloadProgress.value = totalBytes > 0
            ? Math.min(Math.round((downloaded / totalBytes) * 100), 99)
            : 0;
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
