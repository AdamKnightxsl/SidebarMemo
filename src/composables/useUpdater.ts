import { ref } from "vue";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";

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
      // 下载失败（通常是网络问题），用浏览器打开 Release 页面手动下载
      console.error("在线更新失败，尝试浏览器下载:", e);
      const ver = target.version || updateVersion.value;
      const releaseUrl = `https://github.com/AdamKnightxsl/SidebarMemo/releases/tag/v${ver}`;
      try {
        await openUrl(releaseUrl);
        lastError.value = "已打开浏览器下载页面";
      } catch {
        lastError.value = "网络错误，请手动下载";
      }
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
