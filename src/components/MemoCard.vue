<script setup lang="ts">
import { ref, nextTick, onMounted, onBeforeUnmount, computed, inject, watch } from "vue";
import { useMemos, type Memo, type ShowToastFn } from "../composables/useMemos";
import { useThumbnailCache } from "../composables/useThumbnailCache";
import { highlightInHtml } from "../composables/pinyinSearch";
import { sanitizeHtml } from "../composables/sanitizeHtml";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import { usePopupPosition } from "../composables/usePopupPosition";
import { useClickOutside } from "../composables/useClickOutside";

// 配置 marked
marked.setOptions({ breaks: true, gfm: true });

const props = defineProps<{
  memo: Memo;
  selected?: boolean;
}>();

const {
  updateMemo,
  moveToTrash,
  togglePin,
  setReminder,
  clearReminder,
  setColor,
  toggleDone,
  memos,
  reorderMemos,
  saveImage,
  deleteImage,
  getImageAssetUrl,
  searchQuery,
} = useMemos();

const showToast = inject<ShowToastFn>('showToast', (msg: string) => console.warn(msg));

const editing = ref(false);
const editText = ref("");
const editArea = ref<HTMLTextAreaElement | null>(null);
const showColorPicker = ref(false);
const showReminderMenu = ref(false);
const showCustomTime = ref(false);
const customTimeValue = ref("");
const deleting = ref(false);
const cardRef = ref<HTMLDivElement | null>(null);
const colorBtnRef = ref<HTMLButtonElement | null>(null);
const reminderBtnRef = ref<HTMLButtonElement | null>(null);
const expanded = ref(false);

// —— 图片加载失败状态 ——
// 记录加载失败的 filename（CSP 误配、文件丢失等场景），模板据此切到失败占位，
// 避免破图被静默吞掉。重新加载图片（src 变化）时在 loadThumbnailImages / startEdit 清除标记。
const brokenImages = ref<Set<string>>(new Set());

function markImageBroken(filename: string) {
  // src 尚未加载（空字符串）不算失败，避免异步加载期间误标记
  const thumb = getThumb(props.memo.id, filename);
  const url = thumbnailUrls.value.get(filename)
    || editImageUrls.value.get(filename);
  if (!thumb && !url) return;
  if (!brokenImages.value.has(filename)) {
    brokenImages.value = new Set(brokenImages.value).add(filename);
  }
}

function clearBrokenImages() {
  if (brokenImages.value.size > 0) brokenImages.value = new Set();
}

// —— Image editing state ——
const editImages = ref<string[]>([]);
const editImageUrls = ref<Map<string, string>>(new Map());
const fileInputRef = ref<HTMLInputElement | null>(null);
const imageExpandedAnim = ref(false);
let clickTimer: ReturnType<typeof setTimeout> | null = null;

// —— Popup positions (fixed, relative to viewport) ——
const { popupStyle: colorPickerStyle, updatePosition: computeColorPickerPos } = usePopupPosition(colorBtnRef);
const { popupStyle: reminderMenuStyle, updatePosition: computeReminderMenuPos } = usePopupPosition(reminderBtnRef);

useClickOutside({
  ignore: [".color-picker-popup", ".color-btn"],
  onClickOutside: () => { if (showColorPicker.value) showColorPicker.value = false; },
});
useClickOutside({
  ignore: [".reminder-menu", ".reminder-btn"],
  onClickOutside: () => { if (showReminderMenu.value) showReminderMenu.value = false; },
});

// —— Memo images parsed ——
const memoImages = computed(() => {
  try {
    const arr = JSON.parse(props.memo.images || "[]");
    return Array.isArray(arr) ? arr : [];
  } catch { return []; }
});

// Markdown rendering
const renderedContent = computed(() => {
  return marked.parse(props.memo.content || '') as string;
});

// Truncated for collapsed view
const truncatedContent = computed(() => {
  const raw = props.memo.content || '';
  const lines = raw.split('\n').filter(l => l.trim());
  const truncated = lines.slice(0, 4).join('\n');
  return lines.length > 4 ? truncated + '...' : truncated;
});

// Search highlight（支持拼音匹配高亮）
const isSearching = computed(() => searchQuery.value.trim().length > 0);

const displayedContent = computed(() => {
  // 显式读取 searchQuery 确保 Vue 追踪依赖
  const q = searchQuery.value.trim();
  // 搜索时展开显示全部内容
  const rawHtml = (expanded.value || q.length > 0)
    ? renderedContent.value
    : marked.parse(truncatedContent.value) as string;
  // 统一净化，剥离脚本/事件处理属性，防止 v-html 注入执行
  const finalHtml = q ? highlightInHtml(rawHtml, q) : rawHtml;
  return sanitizeHtml(finalHtml);
});

// —— Click to expand / Double-click to edit ——
function handleContentClick() {
  if (clickTimer) {
    clearTimeout(clickTimer);
    clickTimer = null;
    return;
  }
  clickTimer = setTimeout(() => {
    clickTimer = null;
    if (!editing.value) {
      expanded.value = !expanded.value;
    }
  }, 250);
}

function handleContentDblClick() {
  if (clickTimer) {
    clearTimeout(clickTimer);
    clickTimer = null;
  }
  startEdit();
  expanded.value = false;
}

// —— Edit ——————————————————————————————
function autoResize() {
  if (!editArea.value) return;
  editArea.value.style.height = "auto";
  editArea.value.style.height = editArea.value.scrollHeight + "px";
}

async function startEdit() {
  if (props.memo.is_done) return;
  editing.value = true;
  expanded.value = false;
  editText.value = props.memo.content;

  // Initialize edit images from memo
  const images = memoImages.value;
  editImages.value = [...images];
  editImageUrls.value = new Map();
  clearBrokenImages();

  nextTick(() => {
    editArea.value?.focus();
    editArea.value?.setSelectionRange(
      editText.value.length,
      editText.value.length
    );
    autoResize();
  });

  // 编辑态小图优先走缩略图缓存秒显；未命中的才异步加载原图兜底并补建缓存
  for (const filename of images) {
    if (getThumb(props.memo.id, filename)) continue;
    const assetUrl = await getImageAssetUrl(props.memo.id, filename);
    if (assetUrl) {
      editImageUrls.value.set(filename, assetUrl);
      editImageUrls.value = new Map(editImageUrls.value);
      ensureThumb(props.memo.id, filename, assetUrl);
    }
  }
}

function stopEdit() {
  if (!editing.value) return;
  editing.value = false;
  if (editText.value.trim() !== props.memo.content) {
    updateMemo(props.memo.id, editText.value.trim());
  }
  // Clear blob URLs
  editImageUrls.value.forEach((url) => {
    if (url.startsWith('blob:')) URL.revokeObjectURL(url);
  });
  editImageUrls.value = new Map();
}

function handleEditKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    editing.value = false;
  }
}

// —— Image upload ——
function triggerFileInput() {
  fileInputRef.value?.click();
}

function generateFilename(original: string): string {
  const ext = original.split('.').pop()?.toLowerCase() || 'png';
  const uuid = crypto.randomUUID();
  return `${uuid}.${ext}`;
}

async function handleFileSelect(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  input.value = '';

  if (file.size > 20 * 1024 * 1024) {
    showToast("图片大小不能超过 20MB", 3000);
    return;
  }

  const filename = generateFilename(file.name);
  const reader = new FileReader();
  reader.onload = async () => {
    const dataUrl = reader.result as string;
    const base64 = dataUrl.split(',')[1];
    const result = await saveImage(props.memo.id, filename, base64);
    if (result) {
      editImages.value.push(filename);
      editImageUrls.value.set(filename, dataUrl);
      editImageUrls.value = new Map(editImageUrls.value);
      ensureThumb(props.memo.id, filename, dataUrl);
    }
  };
  reader.readAsDataURL(file);
}

async function handleImageRemove(filename: string) {
  await deleteImage(props.memo.id, filename);
  evictThumb(props.memo.id, filename);
  editImages.value = editImages.value.filter((f) => f !== filename);
  const url = editImageUrls.value.get(filename);
  if (url && url.startsWith('blob:')) URL.revokeObjectURL(url);
  editImageUrls.value.delete(filename);
  editImageUrls.value = new Map(editImageUrls.value);
}

// —— Paste image from clipboard ——
async function handlePaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items;
  if (!items) return;

  for (const item of items) {
    if (item.kind === 'file' && item.type.startsWith('image/')) {
      e.preventDefault();
      const file = item.getAsFile();
      if (!file) continue;

      if (file.size > 20 * 1024 * 1024) {
        showToast("图片大小不能超过 20MB", 3000);
        return;
      }

      const ext = file.type.split('/')[1] || 'png';
      const filename = generateFilename(`paste.${ext}`);
      const reader = new FileReader();
      reader.onload = async () => {
        const dataUrl = reader.result as string;
        const base64 = dataUrl.split(',')[1];
        const result = await saveImage(props.memo.id, filename, base64);
        if (result) {
          editImages.value.push(filename);
          editImageUrls.value.set(filename, dataUrl);
          editImageUrls.value = new Map(editImageUrls.value);
          ensureThumb(props.memo.id, filename, dataUrl);
        }
      };
      reader.readAsDataURL(file);
      break;
    }
  }
}

// —— Image URLs (thumbnail + expanded) ——
// thumbnailUrls 存原图 dataURL（展开态使用）；折叠态缩略图优先走模块级缓存，
// 卡片重挂载时缓存秒显示，避免大图重新加载造成的视觉割裂
const { getThumb, ensureThumb, evictThumb } = useThumbnailCache();
const thumbnailUrls = ref<Map<string, string>>(new Map());

async function loadThumbnailImages() {
  clearBrokenImages();
  const urls = new Map<string, string>();
  for (const filename of memoImages.value) {
    const assetUrl = await getImageAssetUrl(props.memo.id, filename);
    if (assetUrl) {
      urls.set(filename, assetUrl);
      // 生成/补全折叠态缩略图缓存（已命中则跳过）
      ensureThumb(props.memo.id, filename, assetUrl);
    }
  }
  thumbnailUrls.value = urls;
}

// —— Image viewer ——
let lastImageOpenTime = 0;

async function openImageViewer(index: number) {
  const now = Date.now();
  if (now - lastImageOpenTime < 500) return;
  lastImageOpenTime = now;

  const filenames = [...memoImages.value];
  if (filenames.length === 0) {
    showToast("没有可预览的图片");
    return;
  }
  const safeIndex = Math.max(0, Math.min(index, filenames.length - 1));

  try {
    // 只发文件名引用，viewer 窗口按需加载图片数据（避免大体积 base64 跨 IPC 传输）
    await invoke("open_image_viewer", {
      payload: {
        memoId: props.memo.id,
        filenames,
        index: safeIndex,
      },
    });
  } catch (e) {
    showToast("打开图片失败: " + String(e));
    console.error("open_image_viewer failed:", e);
  }
}

// —— Color picker ——
function toggleColorPicker(e: Event) {
  e.stopPropagation();
  showColorPicker.value = !showColorPicker.value;
  showReminderMenu.value = false;
  showCustomTime.value = false;
  if (showColorPicker.value) {
    nextTick(() => computeColorPickerPos());
  }
}

function selectColor(color: string) {
  setColor(props.memo.id, props.memo.color === color ? "" : color);
  showColorPicker.value = false;
}

function pad(n: number) {
  return n.toString().padStart(2, "0");
}

function toDbDate(date: Date) {
  return date.getFullYear() + "-" + pad(date.getMonth() + 1) + "-" + pad(date.getDate()) + " " + pad(date.getHours()) + ":" + pad(date.getMinutes()) + ":00";
}

function addMinutes(minutes: number) {
  const date = new Date();
  date.setMinutes(date.getMinutes() + minutes);
  return toDbDate(date);
}

function tomorrowMorning() {
  const date = new Date();
  date.setDate(date.getDate() + 1);
  date.setHours(9, 0, 0, 0);
  return toDbDate(date);
}

function toggleReminderMenu(e: Event) {
  e.stopPropagation();
  if (props.memo.is_done) return;
  showReminderMenu.value = !showReminderMenu.value;
  showColorPicker.value = false;
  showCustomTime.value = false;
  if (showReminderMenu.value) {
    nextTick(() => computeReminderMenuPos());
  }
}

function setQuickReminder(remindAt: string) {
  setReminder(props.memo.id, remindAt);
  showReminderMenu.value = false;
}

function showCustomTimeInput() {
  showCustomTime.value = true;
  const now = new Date();
  now.setMinutes(now.getMinutes() + 30);
  customTimeValue.value = now.getFullYear() + "-" + pad(now.getMonth() + 1) + "-" + pad(now.getDate()) + "T" + pad(now.getHours()) + ":" + pad(now.getMinutes());
}

function confirmCustomReminder() {
  if (!customTimeValue.value) return;
  const normalized = customTimeValue.value.replace("T", " ") + ":00";
  setQuickReminder(normalized);
  showCustomTime.value = false;
  showReminderMenu.value = false;
}

function cancelReminder() {
  clearReminder(props.memo.id);
  showReminderMenu.value = false;
}

// —— Delete ————————————————————————————
function handleDelete() {
  deleting.value = true;
  setTimeout(() => {
    moveToTrash(props.memo.id);
  }, 250);
}

// —— Global click to close expanded / stop flash / stop editing ——
function onGlobalClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.closest(".memo-card")) {
    target.closest(".memo-card")?.classList.remove("reminder-flash");
  }
  if (expanded.value) {
    if (!target.closest(".memo-content") && !target.closest(".memo-images-expanded")) {
      expanded.value = false;
    }
  }
  if (editing.value) {
    if (!target.closest(".memo-card.editing")) {
      stopEdit();
    }
  }
}

// —— Drag & Drop (magnetic snap) ——
let dragClone: HTMLElement | null = null;
let dragSourceEl: HTMLElement | null = null;
let startY = 0;
let dragSourceId = "";
const isDragging = ref(false);
let cloneHeight = 0;

function onMouseDown(e: MouseEvent) {
  if (editing.value) return;
  if (e.button !== 0) return;
  const target = e.target as HTMLElement;
  if (!target.closest(".drag-handle")) return;
  e.preventDefault();

  dragSourceId = props.memo.id;
  dragSourceEl = cardRef.value;
  if (!dragSourceEl) return;

  startY = e.clientY;

  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
}

function onMouseMove(e: MouseEvent) {
  if (!isDragging.value) {
    if (Math.abs(e.clientY - startY) > 5) {
      isDragging.value = true;
      if (clickTimer) {
        clearTimeout(clickTimer);
        clickTimer = null;
      }
      document.body.classList.add("is-dragging");
      createDragClone(e);
    }
    return;
  }

  if (dragClone && dragSourceEl) {
    const dy = e.clientY - startY;
    dragClone.style.transform = `translateY(${dy}px)`;

    checkSnap(e.clientY);
    checkDragOver(e.clientY);
  }
}

function checkSnap(mouseY: number) {
  const fromMemo = memos.value.find((m) => m.id === dragSourceId);
  if (!fromMemo) return;

  const sameGroup = memos.value.filter((m) => m.is_pinned === fromMemo.is_pinned);
  const currentIdx = sameGroup.findIndex((m) => m.id === dragSourceId);
  if (currentIdx === -1) return;

  const targets: { id: string; idx: number }[] = [];
  if (currentIdx > 0) targets.push({ id: sameGroup[currentIdx - 1].id, idx: currentIdx - 1 });
  if (currentIdx < sameGroup.length - 1) targets.push({ id: sameGroup[currentIdx + 1].id, idx: currentIdx + 1 });

  for (const t of targets) {
    const el = document.querySelector<HTMLElement>(`.memo-card[data-memo-id="${t.id}"]`);
    if (!el) continue;
    const rect = el.getBoundingClientRect();
    const dy = mouseY - startY;

    if (t.idx > currentIdx) {
      const cloneBottom = startY + dy + cloneHeight;
      const threshold = rect.top + rect.height;
      if (cloneBottom > threshold) {
        doReorder(dragSourceId, t.id);
        break;
      }
    } else {
      const cloneTop = startY + dy;
      const threshold = rect.bottom - rect.height;
      if (cloneTop < threshold) {
        doReorder(dragSourceId, t.id);
        break;
      }
    }
  }
}

function checkDragOver(mouseY: number) {
  if (!dragClone) return;
  const cloneRect = dragClone.getBoundingClientRect();
  document.querySelectorAll<HTMLElement>(".memo-card[data-memo-id]").forEach((card) => {
    if (card.dataset.memoId === dragSourceId) return;
    const rect = card.getBoundingClientRect();
    const overlap = cloneRect.top < rect.bottom && cloneRect.bottom > rect.top;
    card.classList.toggle("drag-over", overlap);
  });
}

function createDragClone(e: MouseEvent) {
  if (!dragSourceEl) return;
  dragClone = dragSourceEl.cloneNode(true) as HTMLElement;
  const rect = dragSourceEl.getBoundingClientRect();
  cloneHeight = rect.height;
  dragClone.classList.add("drag-clone");
  dragClone.style.position = "fixed";
  dragClone.style.left = rect.left + "px";
  dragClone.style.top = rect.top + "px";
  dragClone.style.width = rect.width + "px";
  dragClone.style.zIndex = "9999";
  dragClone.style.pointerEvents = "none";
  dragClone.style.transition = "none";
  document.body.appendChild(dragClone);
  dragSourceEl.classList.add("drag-placeholder");
}

function onMouseUp(e: MouseEvent) {
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);

  if (dragClone) {
    dragClone.remove();
    dragClone = null;
  }
  if (dragSourceEl) {
    dragSourceEl.classList.remove("drag-placeholder");
    dragSourceEl = null;
  }
  isDragging.value = false;
  dragSourceId = "";
  document.body.classList.remove("is-dragging");
  document.querySelectorAll(".memo-card.drag-over").forEach((el) => el.classList.remove("drag-over"));
}

function doReorder(fromId: string, toId: string) {
  const fromMemo = memos.value.find((m) => m.id === fromId);
  const toMemo = memos.value.find((m) => m.id === toId);
  if (!fromMemo || !toMemo) return;
  if (fromMemo.is_pinned !== toMemo.is_pinned) return;

  const sameGroup = memos.value.filter((m) => m.is_pinned === toMemo.is_pinned);
  const fromIdx = sameGroup.findIndex((m) => m.id === fromId);
  const toIdx = sameGroup.findIndex((m) => m.id === toId);
  if (fromIdx === -1 || toIdx === -1) return;

  const newIds = sameGroup.map((m) => m.id);
  const [removed] = newIds.splice(fromIdx, 1);
  newIds.splice(toIdx, 0, removed);

  const pinnedIds = memos.value.filter((m) => m.is_pinned).map((m) => m.id);
  const unpinnedIds = memos.value.filter((m) => !m.is_pinned).map((m) => m.id);

  const fullIds = toMemo.is_pinned
    ? [...newIds, ...unpinnedIds]
    : [...pinnedIds, ...newIds];

  reorderMemos(fullIds);
  const reordered = fullIds.map((id) => memos.value.find((m) => m.id === id)!).filter(Boolean);
  memos.value.splice(0, memos.value.length, ...reordered);
}

onMounted(() => {
  document.addEventListener("click", onGlobalClick);
  loadThumbnailImages();
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onGlobalClick);
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);
});

function formatTime(t: string) {
  if (!t) return "";
  return t.replace("T", " ").slice(0, 16);
}

// —— Watch expanded to trigger animation ——

watch(expanded, (val) => {
  if (val && memoImages.value.length > 0) {
    imageExpandedAnim.value = false;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        imageExpandedAnim.value = true;
      });
    });
  } else {
    imageExpandedAnim.value = false;
  }
});

// —— Watch memoImages to reload thumbnails ——
watch(memoImages, () => {
  loadThumbnailImages();
});
</script>

<template>
  <div
    ref="cardRef"
    class="memo-card"
    :class="{
      'is-done': memo.is_done,
      editing: editing,
      deleting: deleting,
      'is-selected': selected,
      'has-images': memoImages.length > 0,
    }"
    :data-color="memo.color || undefined"
    :data-memo-id="memo.id"
    @mousedown="onMouseDown"
  >
    <div class="memo-header">
      <div class="memo-header-left">
        <span
          class="drag-handle"
          title="拖拽排序"
        >⋮</span>
        <span class="memo-time">
          <span class="memo-date">{{ memo.created_at.slice(0, 10) }}</span>
          <span class="memo-clock">{{ memo.created_at.slice(11, 16) }}</span>
        </span>
      </div>
      <div class="memo-header-actions">
        <div class="memo-actions">
        <button
          class="memo-action-btn"
          @mousedown.stop
          @click.stop="togglePin(memo.id)"
          :title="memo.is_pinned ? '取消置顶' : '置顶'"
        >📌</button>
        <button
          ref="colorBtnRef"
          class="memo-action-btn color-btn"
          @mousedown.stop
          @click.stop="toggleColorPicker"
          title="标记颜色"
        >🎨</button>
        <button
          class="memo-action-btn"
          @mousedown.stop
          @click.stop="toggleDone(memo.id)"
          :title="memo.is_done ? '标记未完成' : '标记完成'"
        >✓</button>
        <button
          ref="reminderBtnRef"
          v-if="!memo.remind_at"
          class="memo-action-btn reminder-btn"
          @mousedown.stop
          @click.stop="toggleReminderMenu"
          :title="memo.is_done ? '已完成的备忘录无需提醒' : '设置提醒'"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="13" r="7"/>
            <path d="M12 10v4l2.5 1.5"/>
            <path d="M5 4 3 6"/>
            <path d="M19 4l2 2"/>
          </svg>
        </button>
        <button
          ref="reminderBtnRef"
          v-if="memo.remind_at"
          class="memo-action-btn reminder-btn active"
          @mousedown.stop
          @click.stop="toggleReminderMenu"
          :title="memo.is_done ? '已完成的备忘录无需提醒' : '修改提醒'"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="13" r="7"/>
            <path d="M12 10v4l2.5 1.5"/>
            <path d="M5 4 3 6"/>
            <path d="M19 4l2 2"/>
          </svg>
        </button>
        <button
          class="memo-action-btn delete-btn"
          @mousedown.stop
          @click.stop="handleDelete"
          title="删除"
        ><span style="font-size: 20px;">×</span></button>
      </div>
      </div>
    </div>

    <Teleport to="body">
    <div
      v-if="showReminderMenu"
      class="reminder-menu"
      :style="reminderMenuStyle"
      @click.stop
    >
      <button @click="setQuickReminder(addMinutes(10))">10 分钟后</button>
      <button @click="setQuickReminder(addMinutes(30))">30 分钟后</button>
      <button @click="setQuickReminder(addMinutes(60))">1 小时后</button>
      <button @click="setQuickReminder(tomorrowMorning())">明天 9:00</button>
      <button v-if="!showCustomTime" @click="showCustomTimeInput">自定义时间</button>
      <div v-if="showCustomTime" class="custom-time-row">
        <input
          type="datetime-local"
          v-model="customTimeValue"
          class="custom-time-input"
        />
        <button @click="confirmCustomReminder" class="custom-time-confirm">确定</button>
      </div>
      <button v-if="memo.remind_at" class="danger" @click="cancelReminder">取消提醒</button>
    </div>
    </Teleport>

    <!-- Color picker popup -->
    <Teleport to="body">
    <div
      v-if="showColorPicker"
      class="color-picker-popup"
      :style="colorPickerStyle"
      @click.stop
    >
      <div
        v-for="c in ['pink', 'blue', 'green', 'yellow']"
        :key="c"
        class="color-dot"
        :class="{ active: memo.color === c }"
        :data-c="c"
        @click="selectColor(c)"
      ></div>
    </div>
    </Teleport>

    <!-- Content + Thumbnail layout (collapsed) -->
    <div v-if="!editing" class="memo-body">
      <div
        class="memo-content markdown-body"
        :class="{ expanded: expanded || isSearching }"
        :key="'c-' + memo.id + '-' + searchQuery + '-' + expanded"
        @click="handleContentClick"
        @dblclick="handleContentDblClick"
        v-html="displayedContent"

            ></div>

      <!-- Right side thumbnail (collapsed only) -->
      <div
        v-if="memoImages.length > 0 && !expanded"
        class="memo-thumbnail-area"
      >
        <div class="memo-thumbnail-stack">
          <template v-for="(filename, i) in memoImages.slice(0, 2)" :key="filename">
            <img
              v-if="!brokenImages.has(filename)"
              :src="getThumb(memo.id, filename) || thumbnailUrls.get(filename) || ''"
              class="memo-thumbnail"
              :style="{ zIndex: 2 - i, marginLeft: i > 0 ? '-12px' : '0' }"
              @click.stop="openImageViewer(i)"
              @error="markImageBroken(filename)"
            />
            <div
              v-else
              class="memo-thumbnail is-broken"
              :style="{ zIndex: 2 - i, marginLeft: i > 0 ? '-12px' : '0' }"
              @click.stop="openImageViewer(i)"
              title="图片加载失败"
            >!</div>
          </template>
          <span v-if="memoImages.length > 2" class="memo-thumbnail-more">
            +{{ memoImages.length - 2 }}
          </span>
        </div>
      </div>
    </div>

    <!-- Expanded images -->
    <div v-if="!editing && expanded && memoImages.length > 0" class="memo-images-expanded">
      <template v-for="(filename, i) in memoImages" :key="filename">
        <img
          v-if="!brokenImages.has(filename)"
          :src="thumbnailUrls.get(filename) || ''"
          class="memo-expanded-img"
          :class="{ 'anim-visible': imageExpandedAnim }"
          @click.stop="openImageViewer(i)"
          style="cursor: pointer"
          @error="markImageBroken(filename)"
        />
        <div
          v-else
          class="memo-expanded-img is-broken"
          :class="{ 'anim-visible': imageExpandedAnim }"
          @click.stop="openImageViewer(i)"
        >图片加载失败</div>
      </template>
    </div>

    <!-- Edit area -->
    <template v-if="editing">
      <textarea
        ref="editArea"
        class="memo-content-edit"
        v-model="editText"
        @input="autoResize"
        @keydown="handleEditKeydown"
        @paste="handlePaste"
        @blur="stopEdit"
      ></textarea>

      <!-- Image editing area -->
      <div class="edit-images-area" v-if="true">
        <div class="edit-images-row">
          <div
            v-for="filename in editImages"
            :key="filename"
            class="edit-image-thumb"
          >
            <img
              v-if="!brokenImages.has(filename)"
              :src="getThumb(memo.id, filename) || editImageUrls.get(filename) || ''"
              @error="markImageBroken(filename)"
            />
            <div v-else class="edit-image-broken">!</div>
            <button class="edit-image-remove" @mousedown.prevent @click.stop="handleImageRemove(filename)" title="删除图片">×</button>
          </div>
          <button class="edit-image-add" @mousedown.prevent @click.stop="triggerFileInput" title="添加图片">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
          </button>
        </div>
      </div>
      <input
        ref="fileInputRef"
        type="file"
        accept="image/jpeg,image/png,image/webp,image/gif"
        style="display: none"
        @change="handleFileSelect"
      />
      <div class="edit-paste-hint">Ctrl+V 粘贴图片</div>
    </template>
  </div>
</template>
