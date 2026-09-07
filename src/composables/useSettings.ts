import { ref, inject } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { ShowToastFn } from "./useMemos";
import { invokeWithRetry, type MemoTemplate } from "@/utils";

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
  /** 未置顶便签多少天未修改后自动进垃圾桶，0 = 关闭。与 Rust 端 default 一致 */
  auto_trash_days?: number;
  /** 开机自启，默认关 */
  auto_start?: boolean;
  /** 正文模板。null = 从没改过，前端用内置默认；空数组 = 用户自己删光了 */
  templates?: MemoTemplate[] | null;
}

// 模块级单例。同时以命名导出暴露，供 setup 之外的模块（useTour 的文案）只读取值，
// 否则只能在组件里调 useSettings()，在组件外调用会触发 inject 警告。
export const settings = ref<Settings>({
  shortcut: "Alt+M",
  theme: "dark",
  skin: "",
  always_on_top: true,
  note_shortcut: "Alt+N",
  auto_trash_days: 3,
  auto_start: false,
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

  /** 后端会把天数钳到合法区间并返回生效值，本地必须按返回值回填 */
  async function saveAutoTrashDays(days: number) {
    try {
      const applied = await invoke<number>("set_auto_trash_days", { days });
      settings.value.auto_trash_days = applied;
    } catch (e) {
      toast(String(e));
    }
  }

  /** 开机自启由后端先写注册表、成功后才落盘，所以失败时保持原状态展示 */
  async function saveAutoStart(enabled: boolean) {
    try {
      const applied = await invoke<boolean>("set_auto_start", { enabled });
      settings.value.auto_start = applied;
      return applied;
    } catch (e) {
      toast(String(e));
      return settings.value.auto_start ?? false;
    }
  }

  /** 模板整表覆盖保存。后端拒收时不改本地状态，返回 false 让设置页把草稿回滚 */
  async function saveTemplates(list: MemoTemplate[]) {
    try {
      await invoke("set_templates", { templates: list });
      settings.value.templates = list;
      return true;
    } catch (e) {
      toast(String(e));
      return false;
    }
  }

  return {
    settings,
    loadSettings,
    saveShortcut,
    saveTheme,
    saveSkin,
    saveNoteShortcut,
    saveAutoTrashDays,
    saveAutoStart,
    saveTemplates,
  };
}
