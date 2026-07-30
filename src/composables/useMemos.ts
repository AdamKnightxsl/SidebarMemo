import { ref, computed, inject } from "vue";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { matchesQuery } from "./pinyinSearch";

export type ShowToastFn = (msg: string, duration?: number) => void;

export interface Memo {
  id: string;
  content: string;
  created_at: string;
  updated_at: string;
  color: string;
  is_pinned: boolean;
  is_done: boolean;
  sort_order: number;
  is_trashed: boolean;
  trashed_at: string;
  remind_at: string;
  images: string;
}

const memos = ref<Memo[]>([]);
const trashedMemos = ref<Memo[]>([]);
const searchQuery = ref("");
const dateFilter = ref<"all" | "today" | "yesterday" | "day_before_yesterday" | "trash">("all");
const now = ref(Date.now());

let tickTimer: ReturnType<typeof setInterval> | null = null;

function startTicker() {
  tickTimer = setInterval(() => { now.value = Date.now(); }, 60000);
}

function stopTicker() {
  if (tickTimer) { clearInterval(tickTimer); tickTimer = null; }
}

function getMidnight(date: Date): number {
  const d = new Date(date);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

function matchDateFilter(m: Memo): boolean {
  if (dateFilter.value === "all") return true;
  if (dateFilter.value === "trash") return false;
  const memoTime = new Date(m.created_at.replace(" ", "T")).getTime();
  const todayMidnight = getMidnight(new Date(now.value));
  const yesterdayMidnight = todayMidnight - 86400000;
  if (dateFilter.value === "today") return memoTime >= todayMidnight;
  if (dateFilter.value === "yesterday") return memoTime >= yesterdayMidnight && memoTime < todayMidnight;
  const dayBeforeMidnight = yesterdayMidnight - 86400000;
  return memoTime >= dayBeforeMidnight && memoTime < yesterdayMidnight;
}

export function useMemos() {
  if (!tickTimer) startTicker();
  const toast = inject<ShowToastFn>("showToast", () => {});

  const pinnedMemos = computed(() => {
    const q = searchQuery.value.trim();
    let list = memos.value.filter((m) => m.is_pinned && matchDateFilter(m));
    if (q) {
      list = list.filter((m) => matchesQuery(m.content, q));
    }
    return list;
  });

  const unpinnedMemos = computed(() => {
    const q = searchQuery.value.trim();
    let list = memos.value.filter((m) => !m.is_pinned && matchDateFilter(m));
    if (q) {
      list = list.filter((m) => matchesQuery(m.content, q));
    }
    return list;
  });

  async function loadMemos() {
    try {
      memos.value = await invoke<Memo[]>("get_memos");
    } catch (e) {
      toast(String(e));
    }
  }

  async function loadTrashedMemos() {
    try {
      trashedMemos.value = await invoke<Memo[]>("get_trashed_memos");
    } catch (e) {
      toast(String(e));
    }
  }

  async function addMemo(content: string) {
    try {
      const memo = await invoke<Memo>("add_memo", { content });
      memos.value.unshift(memo);
    } catch (e) {
      toast(String(e));
    }
  }

  async function updateMemo(id: string, content: string) {
    try {
      await invoke("update_memo", { id, content });
      const m = memos.value.find((m) => m.id === id);
      if (m) {
        m.content = content;
        m.updated_at = new Date().toISOString().replace("T", " ").slice(0, 19);
      }
    } catch (e) {
      toast(String(e));
    }
  }

  async function deleteMemo(id: string) {
    try {
      await invoke("delete_memo", { id });
      memos.value = memos.value.filter((m) => m.id !== id);
    } catch (e) {
      toast(String(e));
    }
  }

  async function togglePin(id: string) {
    try {
      await invoke("toggle_pin", { id });
      const m = memos.value.find((m) => m.id === id);
      if (m) m.is_pinned = !m.is_pinned;
    } catch (e) {
      toast(String(e));
    }
  }

  async function setColor(id: string, color: string) {
    try {
      await invoke("set_color", { id, color });
      const m = memos.value.find((m) => m.id === id);
      if (m) m.color = color;
    } catch (e) {
      toast(String(e));
    }
  }

  async function toggleDone(id: string) {
    try {
      await invoke("toggle_done", { id });
      const m = memos.value.find((m) => m.id === id);
      if (m) m.is_done = !m.is_done;
    } catch (e) {
      toast(String(e));
    }
  }

  async function reorderMemos(ids: string[]) {
    try {
      await invoke("reorder_memos", { ids });
      // 同步前端 sort_order，避免与库不一致
      ids.forEach((id, i) => {
        const m = memos.value.find((m) => m.id === id);
        if (m) m.sort_order = i;
      });
    } catch (e) {
      toast(String(e));
    }
  }

  async function moveToTrash(id: string) {
    try {
      await invoke("move_to_trash", { id });
      const m = memos.value.find((m) => m.id === id);
      if (m) {
        m.is_trashed = true;
        m.trashed_at = new Date().toISOString().replace("T", " ").slice(0, 19);
      }
      memos.value = memos.value.filter((m) => m.id !== id);
    } catch (e) {
      toast(String(e));
    }
  }

  async function restoreFromTrash(id: string) {
    try {
      await invoke("restore_from_trash", { id });
      trashedMemos.value = trashedMemos.value.filter((m) => m.id !== id);
      await loadMemos();
    } catch (e) {
      toast(String(e));
    }
  }

  async function permanentDeleteMemo(id: string) {
    try {
      await invoke("permanent_delete", { id });
      trashedMemos.value = trashedMemos.value.filter((m) => m.id !== id);
    } catch (e) {
      toast(String(e));
    }
  }

  async function clearTrash() {
    try {
      await invoke("clear_trashed");
      trashedMemos.value = [];
    } catch (e) {
      toast(String(e));
    }
  }

  async function setReminder(id: string, remindAt: string) {
    try {
      await invoke("set_reminder", { id, remindAt });
      const m = memos.value.find((m) => m.id === id);
      if (m) m.remind_at = remindAt;
      toast("提醒已设置", 2000);
    } catch (e) {
      toast(String(e));
    }
  }

  async function clearReminder(id: string) {
    try {
      await invoke("clear_reminder", { id });
      const m = memos.value.find((m) => m.id === id);
      if (m) m.remind_at = "";
      toast("提醒已取消", 2000);
    } catch (e) {
      toast(String(e));
    }
  }

  async function saveImage(id: string, filename: string, dataBase64: string): Promise<string | null> {
    try {
      const imagesJson = await invoke<string>("save_image", { memoId: id, filename, dataBase64 });
      const m = memos.value.find((m) => m.id === id);
      if (m) m.images = imagesJson;
      return imagesJson;
    } catch (e) {
      toast(String(e));
      return null;
    }
  }

  async function deleteImage(id: string, filename: string): Promise<string | null> {
    try {
      const imagesJson = await invoke<string>("delete_image", { memoId: id, filename });
      const m = memos.value.find((m) => m.id === id);
      if (m) m.images = imagesJson;
      return imagesJson;
    } catch (e) {
      toast(String(e));
      return null;
    }
  }

  async function getImageBase64(memoId: string, filename: string): Promise<string | null> {
    try {
      return await invoke<string>("get_image_base64", { memoId, filename });
    } catch (e) {
      toast(String(e));
      return null;
    }
  }

  async function getImageAssetUrl(memoId: string, filename: string): Promise<string | null> {
    try {
      const path = await invoke<string>("get_image_path", { memoId, filename });
      return convertFileSrc(path);
    } catch (e) {
      toast(String(e));
      return null;
    }
  }

  return {
    memos,
    trashedMemos,
    searchQuery,
    dateFilter,
    pinnedMemos,
    unpinnedMemos,
    loadMemos,
    loadTrashedMemos,
    addMemo,
    updateMemo,
    deleteMemo,
    togglePin,
    setColor,
    toggleDone,
    reorderMemos,
    moveToTrash,
    restoreFromTrash,
    permanentDeleteMemo,
    clearTrash,
    setReminder,
    clearReminder,
    saveImage,
    deleteImage,
    getImageBase64,
    getImageAssetUrl,
  };
}
