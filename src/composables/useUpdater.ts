import { ref } from "vue";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

const updating = ref(false);
const updateAvailable = ref(false);
const updateVersion = ref("");
const downloadProgress = ref(0);

export function useUpdater() {
  async function checkForUpdates(silent = true) {
    try {
      const update = await check();
      if (update) {
        updateAvailable.value = true;
        updateVersion.value = update.version || "";
        if (!silent) {
          await installUpdate(update);
        }
        return update;
      }
    } catch (e) {
      if (!silent) {
        console.error("检查更新失败:", e);
      }
    }
    return null;
  }

  async function installUpdate(update: any) {
    if (updating.value) return;
    updating.value = true;
    downloadProgress.value = 0;

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
    } catch (e) {
      console.error("安装更新失败:", e);
      updating.value = false;
    }
  }

  return {
    updating,
    updateAvailable,
    updateVersion,
    downloadProgress,
    checkForUpdates,
    installUpdate,
  };
}
