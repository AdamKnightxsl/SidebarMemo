import { ref, computed, inject } from "vue";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { matchesQuery } from "./pinyinSearch";
import { extractTags, hasTag } from "./memoTags";
import { invokeWithRetry, repeatLabel, type RepeatValue } from "@/utils";

export type ShowToastFn = (
  msg: string,
  duration?: number,
  onClick?: () => void,
  action?: ToastAction,
) => void;

/** Toast 右侧的一次性动作按钮（撤销、二次确认等）。仅在 duration > 0 时生效 */
export interface ToastAction {
  label: string;
  onClick: () => void;
}

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
  /** 重复提醒规则：'' | daily | weekly | monthly:DD（monthly 由后端补上锚定日） */
  remind_repeat: string;
  /** 归档时间，空字符串表示未归档 */
  archived_at: string;
  images: string;
  /** 所属集合 id，空字符串表示普通便签：不进今/昨，也不参与自动清理 */
  board: string;
}

/** 导航栏上用户自定义的集合按钮，顺序由后端按创建先后返回 */
export interface Board {
  id: string;
  name: string;
  created_at: string;
  /** 归属色键名，见 composables/boardColors.ts；空串＝不着色 */
  color: string;
}

const memos = ref<Memo[]>([]);
const trashedMemos = ref<Memo[]>([]);
/** 归档列表：后端 get_memos 不返回归档条目，所以单独拉取 */
const archivedMemos = ref<Memo[]>([]);
const searchQuery = ref("");
const colorFilter = ref<string[]>([]);
/** 按正文里的 #标签 筛选，多选之间是「或」，与颜色筛选的行为保持一致 */
const tagFilter = ref<string[]>([]);
const dateFilter = ref<"all" | "today" | "yesterday" | "trash" | "archive">("all");
/** 导航栏上用户自定义的集合，顺序即显示顺序（老的在前、新建的紧挨 + 号） */
const boards = ref<Board[]>([]);
/** 当前打开的集合 id，空串表示不在集合视图。与 dateFilter 配合决定主列表内容 */
const boardFilter = ref("");
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

/** 返回本地时间字符串，格式与后端 Local::now() 一致：YYYY-MM-DD HH:MM:SS */
function localNowStr(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function matchDateFilter(m: Memo): boolean {
  if (dateFilter.value === "trash") return false;
  // 归档视图有独立列表，主列表的按天筛选对它不适用（memos 里本来也不含归档条目）
  if (dateFilter.value === "archive") return false;
  // 集合视图只看当前集合，集合内部不再按日期分区
  if (boardFilter.value) return m.board === boardFilter.value;
  // 集合里的便签是独立内容：只出现在「全部」和它自己的集合里，不进今/昨
  if (m.board) return dateFilter.value === "all";
  const memoTime = new Date(m.created_at.replace(" ", "T")).getTime();
  const todayMidnight = getMidnight(new Date(now.value));
  const yesterdayMidnight = todayMidnight - 86400000;
  if (dateFilter.value === "today") return memoTime >= todayMidnight;
  if (dateFilter.value === "yesterday") return memoTime >= yesterdayMidnight && memoTime < todayMidnight;
  return true;
}

function matchSearch(m: Memo): boolean {
  const q = searchQuery.value.trim();
  return !q || matchesQuery(m.content, q);
}

function matchColorFilter(m: Memo): boolean {
  return colorFilter.value.length === 0 || colorFilter.value.includes(m.color);
}

function matchTagFilter(m: Memo): boolean {
  // 空筛选先短路：标签要扫正文，没选时没必要为每条便签跑一遍正则
  if (tagFilter.value.length === 0) return true;
  return tagFilter.value.some((t) => hasTag(m.content, t));
}

/** 点卡片上的 #标签＝只看这一个标签；再点同一个则取消筛选。返回筛选后是否处于选中 */
export function focusTag(tag: string): boolean {
  const only = tagFilter.value.length === 1 && tagFilter.value[0] === tag;
  tagFilter.value = only ? [] : [tag];
  return !only;
}

/** 标签弹层里的多选切换 */
export function toggleTagFilter(tag: string) {
  const i = tagFilter.value.indexOf(tag);
  if (i >= 0) tagFilter.value.splice(i, 1);
  else tagFilter.value.push(tag);
}

/** 集合 id → 显示名。卡片标签和右键菜单都只想要个名字，不值得为它们走一遍 useMemos() */
export function boardName(id: string): string {
  return boards.value.find((b) => b.id === id)?.name ?? "";
}

/** 集合 id → 归属色键名（空串＝不着色）。同 boardName，只要一个字段 */
export function boardColor(id: string): string {
  return boards.value.find((b) => b.id === id)?.color ?? "";
}

export function useMemos() {
  if (!tickTimer) startTicker();
  const toast = inject<ShowToastFn>("showToast", () => {});

  const pinnedMemos = computed(() =>
    memos.value.filter((m) => m.is_pinned && matchDateFilter(m) && matchSearch(m) && matchColorFilter(m) && matchTagFilter(m))
  );

  const unpinnedMemos = computed(() =>
    memos.value.filter((m) => !m.is_pinned && matchDateFilter(m) && matchSearch(m) && matchColorFilter(m) && matchTagFilter(m))
  );

  /** 列表当前真正渲染出来的顺序（置顶组在前）。拖拽排序必须拿它找邻居：
      用全量 memos 时，一旦颜色/标签/日期筛选把邻居卡片藏掉，DOM 里就查不到节点，拖拽会静默失效 */
  const visibleMemos = computed(() => [...pinnedMemos.value, ...unpinnedMemos.value]);

  /** 各标签的条数，按日期与搜索范围统计（不含标签筛选本身，否则选中后其余标签会显示为 0） */
  const tagCounts = computed(() => {
    const counts: Record<string, number> = {};
    for (const m of memos.value) {
      if (!matchDateFilter(m) || !matchSearch(m)) continue;
      for (const t of extractTags(m.content)) counts[t] = (counts[t] || 0) + 1;
    }
    return counts;
  });

  /** 标签弹层按条数降序，条数相同再按名称，保证顺序稳定不随列表刷新跳动 */
  const tagList = computed(() =>
    Object.keys(tagCounts.value).sort((a, b) => tagCounts.value[b] - tagCounts.value[a] || a.localeCompare(b, "zh-Hans-CN"))
  );

  /** 各颜色标记的条数，按日期与搜索范围统计（不含颜色筛选本身，否则选中后其余颜色会显示为 0） */
  const colorCounts = computed(() => {
    const counts: Record<string, number> = {};
    for (const m of memos.value) {
      if (!m.color || !matchDateFilter(m) || !matchSearch(m)) continue;
      counts[m.color] = (counts[m.color] || 0) + 1;
    }
    return counts;
  });

  async function loadMemos() {
    try {
      memos.value = await invokeWithRetry<Memo[]>(() => invoke<Memo[]>("get_memos"));
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

  async function loadArchivedMemos() {
    try {
      archivedMemos.value = await invoke<Memo[]>("get_archived_memos");
    } catch (e) {
      toast(String(e));
    }
  }

  /** 新建便签。board 传集合 id 则直接落在该集合里，不传或空串为普通便签 */
  async function addMemo(content: string, board = "") {
    try {
      const memo = await invoke<Memo>("add_memo", { content, board });
      memos.value.unshift(memo);
    } catch (e) {
      toast(String(e));
    }
  }

  async function loadBoards() {
    try {
      boards.value = await invoke<Board[]>("get_boards");
    } catch (e) {
      toast(String(e));
    }
  }

  /** 新建集合。名称为空、重名或超过 8 字时后端会拒绝，返回 null，调用方不要切换视图 */
  async function createBoard(name: string, color: string): Promise<Board | null> {
    try {
      const board = await invoke<Board>("create_board", { name, color });
      boards.value.push(board);
      return board;
    } catch (e) {
      toast(String(e));
      return null;
    }
  }

  /** 改名与改色一次提交：弹框里两项是一起确认的，分两条命令会让它们有机会不一致 */
  async function saveBoard(id: string, name: string, color: string): Promise<boolean> {
    try {
      await invoke("update_board", { id, name, color });
    } catch (e) {
      toast(String(e));
      return false;
    }
    const b = boards.value.find((b) => b.id === id);
    if (b) {
      b.name = name.trim();
      b.color = color;
    }
    return true;
  }

  /**
   * 删除集合，集合里的便签连同进垃圾桶（不参与自动清理，所以只能这样手动清）。
   * 导航状态不在这里改：boardFilter 只有 App.vue 会写，调用方删除后自行切回「全部」。
   * @returns 随之下拉圾桶的条数，失败返回 -1
   */
  async function deleteBoard(id: string): Promise<number> {
    try {
      const trashed = await invoke<number>("delete_board", { id });
      boards.value = boards.value.filter((b) => b.id !== id);
      await Promise.all([loadMemos(), loadTrashedMemos()]);
      return trashed;
    } catch (e) {
      toast(String(e));
      return -1;
    }
  }

  /** 把已有便签移进集合（board 传集合 id）或退回普通便签（传空串）。移动不算编辑，不动 updated_at */
  async function setMemoBoard(id: string, board: string): Promise<boolean> {
    try {
      await invoke("set_memo_board", { id, board });
    } catch (e) {
      toast(String(e));
      return false;
    }
    const m = memos.value.find((m) => m.id === id);
    if (!m) return false;
    m.board = board;
    return true;
  }

  async function updateMemo(id: string, content: string): Promise<boolean> {
    try {
      await invoke("update_memo", { id, content });
      const m = memos.value.find((m) => m.id === id);
      if (m) {
        m.content = content;
        m.updated_at = localNowStr();
      }
      return true;
    } catch (e) {
      toast(String(e));
      return false;
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
      if (m) {
        m.color = color;
        m.updated_at = localNowStr();
      }
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

  /** @returns 是否真的删除成功，调用方据此决定是否给出撤销入口 */
  async function moveToTrash(id: string): Promise<boolean> {
    try {
      await invoke("move_to_trash", { id });
      const m = memos.value.find((m) => m.id === id);
      if (m) {
        m.is_trashed = true;
        m.trashed_at = localNowStr();
      }
      memos.value = memos.value.filter((m) => m.id !== id);
      return true;
    } catch (e) {
      toast(String(e));
      return false;
    }
  }

  async function undoReorder(ids: string[]) {
    await reorderMemos(ids);
    // reorderMemos 只回写 sort_order，数组顺序仍是拖完的样子，必须重拉才对得上
    await loadMemos();
  }

  /** 撤销删除。is_pinned / sort_order 删除时没被动过，重拉一次列表就回到原来的位置 */
  async function undoTrash(id: string, remindAt: string) {
    try {
      await invoke("undo_trash", { id, remindAt });
      await loadMemos();
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

  async function setReminder(id: string, remindAt: string, repeat: RepeatValue = "") {
    try {
      await invoke("set_reminder", { id, remindAt, remindRepeat: repeat });
      const m = memos.value.find((m) => m.id === id);
      if (m) {
        m.remind_at = remindAt;
        // 后端把 monthly 归一成 monthly:DD（锚定日取提醒时间的「日」），本地存成同样的形状
        m.remind_repeat = repeat === "monthly" ? `monthly:${remindAt.slice(8, 10)}` : repeat;
      }
      toast(repeat ? `提醒已设置 · ${repeatLabel(repeat)}` : "提醒已设置", 2000);
    } catch (e) {
      toast(String(e));
    }
  }

  async function clearReminder(id: string) {
    try {
      await invoke("clear_reminder", { id });
      const m = memos.value.find((m) => m.id === id);
      if (m) {
        m.remind_at = "";
        m.remind_repeat = "";
      }
      toast("提醒已取消", 2000);
    } catch (e) {
      toast(String(e));
    }
  }

  /**
   * 归档 / 取消归档。归档只改 archived_at，置顶、排序、提醒计划全部保留。
   * 一次操作同时改变主列表和归档列表两边的归属，所以整体重取而不是本地挪动。
   * @returns 是否成功，调用方据此给提示和撤销入口
   */
  async function setArchived(id: string, archived: boolean): Promise<boolean> {
    try {
      await invoke("set_archived", { id, archived });
    } catch (e) {
      toast(String(e));
      return false;
    }
    await Promise.all([loadMemos(), loadArchivedMemos()]);
    return true;
  }

  async function saveImage(id: string, filename: string, dataBase64: string): Promise<string | null> {
    try {
      const imagesJson = await invoke<string>("save_image", { memoId: id, filename, dataBase64 });
      const m = memos.value.find((m) => m.id === id);
      if (m) {
        m.images = imagesJson;
        m.updated_at = localNowStr();
      }
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
      if (m) {
        m.images = imagesJson;
        m.updated_at = localNowStr();
      }
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
    archivedMemos,
    searchQuery,
    colorFilter,
    colorCounts,
    tagFilter,
    tagCounts,
    tagList,
    dateFilter,
    boards,
    boardFilter,
    pinnedMemos,
    unpinnedMemos,
    visibleMemos,
    loadMemos,
    loadTrashedMemos,
    loadArchivedMemos,
    loadBoards,
    createBoard,
    saveBoard,
    deleteBoard,
    setMemoBoard,
    setArchived,
    addMemo,
    updateMemo,
    deleteMemo,
    togglePin,
    setColor,
    toggleDone,
    reorderMemos,
    undoReorder,
    moveToTrash,
    undoTrash,
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
