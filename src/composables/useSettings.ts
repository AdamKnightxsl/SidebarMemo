import { ref, inject } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { ShowToastFn } from "./useMemos";
import { invokeWithRetry } from "@/utils";

export interface Settings {
  shortcut: string;
  theme: string;
  skin: string;
  always_on_top?: boolean;
  note_shortcut?: string;
  window_x?: number | null;
  window_y?: number | null;
  window_width?: number | null;
  window_height?: number | null;
}

const settings = ref<Settings>({
  shortcut: "Alt+M",
  theme: "dark",
  skin: "",
  always_on_top: true,
  note_shortcut: "Alt+N",
} as Settings);

export function useSettings() {
  const toast = inject<ShowToastFn>('showToast', (msg: string) => console.error(msg));
  async function loadSettings() {
    try {
      const s = await invokeWithRetry<Settings>(() => invoke<Settings>("get_settings"));
      invoke("fe_log", { msg: `get_settings ok theme=${s.theme} skin=${s.skin}` }).catch(() => {});
      settings.value = s;
    } catch (e) {
      invoke("fe_log", { msg: `get_settings ERROR ${String(e)}` }).catch(() => {});
      toast(String(e));
    }
  }

  async function saveShortcut(shortcut: string) {
    try {
      await invoke("set_shortcut", { s: shortcut });
      settings.value.shortcut = shortcut;
    } catch (e) {
      toast(String(e));
    }
  }

  async function saveTheme(theme: string) {
    try {
      await invoke("set_theme", { t: theme });
      settings.value.theme = theme;
    } catch (e) {
      toast(String(e));
    }
  }

  async function saveSkin(skin: string) {
    try {
      await invoke("set_skin", { s: skin });
      settings.value.skin = skin;
    } catch (e) {
      toast(String(e));
    }
  }

  async function saveNoteShortcut(shortcut: string) {
    try {
      await invoke("set_note_shortcut", { s: shortcut });
      settings.value.note_shortcut = shortcut;
    } catch (e) {
      toast(String(e));
    }
  }

  return {
    settings,
    loadSettings,
    saveShortcut,
    saveTheme,
    saveSkin,
    saveNoteShortcut,
  };
}
