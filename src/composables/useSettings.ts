import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  shortcut: string;
  theme: string;
  skin: string;
}

const settings = ref<Settings>({} as Settings);

export function useSettings() {
  async function loadSettings() {
    try {
      settings.value = await invoke<Settings>("get_settings");
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }

  async function saveShortcut(shortcut: string) {
    try {
      await invoke("set_shortcut", { s: shortcut });
      settings.value.shortcut = shortcut;
    } catch (e) {
      console.error("Failed to save shortcut:", e);
    }
  }

  async function saveTheme(theme: string) {
    try {
      await invoke("set_theme", { t: theme });
      settings.value.theme = theme;
    } catch (e) {
      console.error("Failed to save theme:", e);
    }
  }

  async function saveSkin(skin: string) {
    try {
      await invoke("set_skin", { s: skin });
      settings.value.skin = skin;
    } catch (e) {
      console.error("Failed to save skin:", e);
    }
  }

  return {
    settings,
    loadSettings,
    saveShortcut,
    saveTheme,
    saveSkin,
  };
}
