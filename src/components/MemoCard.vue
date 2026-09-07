<script setup lang="ts">
import { ref, nextTick, onMounted, onBeforeUnmount, computed, inject, watch } from "vue";
import { useMemos, boardName, boardColor, focusTag, type Memo, type ShowToastFn } from "../composables/useMemos";
import { useThumbnailCache } from "../composables/useThumbnailCache";
import { highlightInHtml } from "../composables/pinyinSearch";
import { markTagsInHtml, tagFromChip, hasTag, normalizeTag } from "../composables/memoTags";
import { markTasksInHtml, toggleTask } from "../composables/memoTasks";
import { sanitizeHtml } from "../composables/sanitizeHtml";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import { usePopupPosition } from "../composables/usePopupPosition";
import { useClickOutside } from "../composables/useClickOutside";
import { useContextMenu } from "../composables/useContextMenu";
import MarkdownToolbar from "../components/MarkdownToolbar.vue";
import { mdToEditorHtml, htmlToMarkdown, isRoundTripSafe } from "../composables/mdSerialize";
import { contentPreview, isComposing, REPEAT_OPTIONS, repeatKey, repeatLabel, type RepeatValue } from "../utils";

// 配置 marked
marked.setOptions({ breaks: true, gfm: true });

const props = defineProps<{
  memo: Memo;
  selected?: boolean;
}>();

const {
  updateMemo,
  moveToTrash,
  undoTrash,
  togglePin,
  setReminder,
  clearReminder,
  setArchived,
  setColor,
  toggleDone,
  memos,
  visibleMemos,
  reorderMemos,
  undoReorder,
  saveImage,
  deleteImage,
  getImageAssetUrl,
  searchQuery,
  boards,
  boardFilter,
  setMemoBoard,
} = useMemos();

const showToast = inject<ShowToastFn>('showToast', (msg: string) => console.warn(msg));
/** 点卡片上的集合标签＝跳到那个集合。只有 App.vue 持有导航状态，所以走 inject 而不是自己改 filter */
const openBoard = inject<((id: string) => void) | null>("openBoard", null);

const editing = ref(false);
const editText = ref("");
const editArea = ref<HTMLTextAreaElement | null>(null);
/** 编辑区形态：正文能安全往返才用所见即所得，否则退回源码 textarea */
const editMode = ref<"wysiwyg" | "source">("wysiwyg");
const editorEl = ref<HTMLDivElement | null>(null);
const showColorPicker = ref(false);
const showReminderMenu = ref(false);
const showCustomTime = ref(false);
const customTimeValue = ref("");
/** 提醒菜单里选中的重复规则；未设提醒时间时只是暂存，选时间那一刻才写库 */
const repeatChoice = ref<RepeatValue>("");
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

// —— 右键上下文菜单 ——
const { openId, pos, open: openMenu, close: closeMenu } = useContextMenu();
const menuOpen = computed(() => openId.value === props.memo.id);
const ctxMenuRef = ref<HTMLDivElement | null>(null);
const ctxStyle = ref<Record<string, string>>({});
/** 菜单二级：就地展开集合列表（弹层窄，做侧向 flyout 容易溢出窗口） */
const ctxBoards = ref(false);
/** 菜单二级：就地展开「添加标签」输入框 */
const ctxTags = ref(false);
const newTag = ref("");
const tagInputRef = ref<HTMLInputElement | null>(null);
/** 集合被删后 id 查不到名字，此时卡片上的集合标签也就不该再显示 */
const memoBoardName = computed(() => boardName(props.memo.board));
const memoBoardColor = computed(() => boardColor(props.memo.board));

function placeContextMenu() {
  const el = ctxMenuRef.value;
  if (!el) return;
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  const left = Math.max(6, Math.min(window.innerWidth - w - 6, pos.value.x));
  const top = Math.max(6, Math.min(window.innerHeight - h - 6, pos.value.y));
  ctxStyle.value = { left: left + "px", top: top + "px" };
}

watch([ctxBoards, ctxTags], () => {
  // 二级列表比一级高，展开后要按新高度重新夹一次，否则底部会被裁掉
  if (menuOpen.value) nextTick(placeContextMenu);
});

function closeOnScroll() {
  closeMenu();
}

watch(menuOpen, (v) => {
  // 菜单位置是固定坐标，列表一滚就会和卡片脱节 → 滚动即收起
  window.removeEventListener("scroll", closeOnScroll, true);
  if (v) {
    nextTick(placeContextMenu);
    window.addEventListener("scroll", closeOnScroll, true);
  }
});

function openContextMenu(e: MouseEvent) {
  // 正在编辑时右键属于输入过程的一部分，不能拿菜单打断光标
  if (editing.value) return;
  e.preventDefault();
  showColorPicker.value = false;
  showReminderMenu.value = false;
  ctxBoards.value = false;
  ctxTags.value = false;
  newTag.value = "";
  openMenu(props.memo.id, e.clientX, e.clientY);
}

useClickOutside({
  ignore: [".memo-context-menu"],
  eventType: "mousedown",
  onClickOutside: () => { if (openId.value) closeMenu(); },
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
  const lines = (props.memo.content || '').split('\n');
  // 空行必须留着：删掉后「- 甲\n\n乙」会变成「- 甲\n乙」，markdown 会把乙并进最后一个列表项，
  // 于是收起态的普通行凭空多出圆点和缩进
  const kept: string[] = [];
  let used = 0;
  let more = false;
  for (const l of lines) {
    if (l.trim()) {
      if (used === 4) { more = true; break; }
      used++;
    }
    kept.push(l);
  }
  while (kept.length && !kept[kept.length - 1].trim()) kept.pop();
  return more ? kept.join('\n') + '...' : kept.join('\n');
});

// Search highlight（支持拼音匹配高亮）
const isSearching = computed(() => searchQuery.value.trim().length > 0);

const displayedContent = computed(() => {
  // 显式读取 searchQuery 确保 Vue 追踪依赖
  const q = searchQuery.value.trim();
  // 搜索时展开显示全部内容
  const collapsed = !expanded.value && q.length === 0;
  const source = collapsed ? truncatedContent.value : props.memo.content || "";
  const rawHtml = collapsed ? (marked.parse(source) as string) : renderedContent.value;
  // 复选框必须在高亮前换成空 span：不引入纯文本，<mark> 的下标才不会错位
  const boxedHtml = markTasksInHtml(rawHtml, source);
  // 先包 #标签再高亮：高亮按纯文本下标插 <mark>，插完标签后纯文本内容不变，位置仍对得上
  const taggedHtml = markTagsInHtml(boxedHtml);
  const finalHtml = q ? highlightInHtml(taggedHtml, q) : taggedHtml;
  // 统一净化，剥离脚本/事件处理属性，防止 v-html 注入执行
  return sanitizeHtml(finalHtml);
});

// —— Click to expand / Double-click to edit ——
function handleContentClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  const taskBox = target.closest?.(".md-task");
  if (taskBox) {
    void toggleTaskItem(taskBox);
    return;
  }
  const chip = target.closest?.(".md-tag");
  if (chip) {
    // 点标签＝按标签筛选，不能顺带展开卡片：展开会换掉整份 HTML，chip 随即消失
    const tag = tagFromChip(chip.textContent);
    if (!tag) return;
    showToast(focusTag(tag) ? `只看 #${tag} 的便签` : "已取消标签筛选", 2500);
    return;
  }
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

/**
 * 勾选／取消第 k 个子任务：k 按 DOM 里 .md-task 的顺序数。
 * 净化白名单没放开 data-*，而 chip 是顺序插入的，DOM 顺序就是正文里的顺序。
 */
async function toggleTaskItem(box: Element) {
  const host = box.closest(".memo-content");
  const chips = host ? Array.from(host.querySelectorAll(".md-task")) : [];
  const index = chips.indexOf(box);
  if (index < 0) return;
  const next = toggleTask(props.memo.content, index);
  if (next === null) return; // 渲染与正文对不上时不改任何字
  await updateMemo(props.memo.id, next);
}

function handleContentDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest?.(".md-task")) return;
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
  // 往返不安全的正文（表格等）一律留在源码模式：宁可让他看到符号，也不能改写他的字
  editMode.value = isRoundTripSafe(props.memo.content) ? "wysiwyg" : "source";

  // Initialize edit images from memo
  const images = memoImages.value;
  editImages.value = [...images];
  editImageUrls.value = new Map();
  clearBrokenImages();

  nextTick(() => {
    if (editMode.value === "wysiwyg") {
      const el = editorEl.value;
      if (!el) return;
      // 只在进入编辑那一刻写一次 innerHTML：之后由浏览器自己维护，Vue 不再重渲染（会吞光标）
      el.innerHTML = mdToEditorHtml(props.memo.content);
      el.focus();
      placeCaretAtEnd(el);
      return;
    }
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

function placeCaretAtEnd(el: HTMLElement) {
  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

/** 保存时把编辑区 DOM 转回 markdown；正文仍只存 markdown，查看态/搜索/导出全都不变 */
function currentEditMarkdown(): string {
  if (editMode.value !== "wysiwyg") return editText.value.trim();
  const el = editorEl.value;
  return el ? htmlToMarkdown(el) : props.memo.content;
}

function stopEdit() {
  if (!editing.value) return;
  editing.value = false;
  const next = currentEditMarkdown();
  if (next !== props.memo.content) {
    updateMemo(props.memo.id, next);
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
    return;
  }
  // 输入法组词期间的回车属于上屏动作，不能接管
  if (isComposing(e) || editMode.value !== "wysiwyg") return;
  if (e.key !== "Enter" || !e.shiftKey || e.ctrlKey || e.metaKey || e.altKey) return;
  if (editorEl.value && leaveListWithBlankLine(editorEl.value)) e.preventDefault();
}

/**
 * 列表项里 Shift+Enter＝离开列表顶格换行（回车仍是继续列表）。
 * 不交给 execCommand 是因为它在 <li> 上的默认行为会留在列表里，
 * 光标落点也认不准，这里手动改 DOM + 显式定位光标。
 */
function leaveListWithBlankLine(root: HTMLElement): boolean {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return false;
  const anchor = sel.anchorNode;
  const from = anchor instanceof Element ? anchor : anchor?.parentElement;
  const li = from?.closest("li");
  if (!li || !root.contains(li)) return false;

  // 嵌套列表整体离开：一路爬到编辑器里最外层的那个列表
  let top = li.parentElement as HTMLElement;
  while (
    top.parentElement &&
    top.parentElement !== root &&
    /^(LI|UL|OL)$/.test(top.parentElement.tagName)
  ) {
    top = top.parentElement;
  }
  const parent = top.parentElement;
  if (!parent) return false;

  let row: HTMLElement = li;
  while (row.parentElement && row.parentElement !== top) row = row.parentElement;

  const line = document.createElement("div");
  line.innerHTML = "<br>";
  if (!(row.textContent || "").trim() && !row.querySelector("img,input")) {
    row.remove();
    if (top.children.length) top.after(line);
    else top.replaceWith(line);
  } else {
    // 这一项之后的行还留在列表里，得另起一个列表，否则会被并进当前项之后
    const tail = document.createElement(top.tagName.toLowerCase());
    const kept = Array.from(top.children).indexOf(row) + 1;
    if (top.tagName === "OL") {
      tail.setAttribute("start", String((Number(top.getAttribute("start")) || 1) + kept));
    }
    let seen = false;
    for (const sib of Array.from(top.children)) {
      if (!seen) {
        if (sib === row) seen = true;
        continue;
      }
      tail.appendChild(sib);
    }
    top.after(line);
    if (tail.children.length) line.after(tail);
  }

  const range = document.createRange();
  range.setStart(line, 0);
  range.collapse(true);
  sel.removeAllRanges();
  sel.addRange(range);
  return true;
}

/**
 * 工具栏只在「一行还塞得下 2×3」时留在图片右侧，被挤下就整条挪到图片上方排一行。
 * 判据用实测宽度而不是图片张数：窗口能拉宽拉窄，同样两张图可能塞得下也可能塞不下。
 */
const imagesRowEl = ref<HTMLElement | null>(null);
const toolbarPlacement = ref<"grid" | "row">("grid");
/** 2×3 工具栏的自然宽度：按钮文案固定，量到一次就够用（flex-shrink:0 保证量的是真实宽度） */
let gridToolbarWidth = 0;

function syncToolbarPlacement() {
  const row = imagesRowEl.value;
  const add = row?.querySelector<HTMLElement>(".edit-image-add");
  if (!row || !add) return;
  const inline = row.querySelector<HTMLElement>(".md-toolbar");
  if (inline?.offsetWidth) gridToolbarWidth = inline.offsetWidth;
  if (!gridToolbarWidth) return;
  // 图片块实测宽度 + 6px 间距 + 工具栏自己的 6px 左边距
  const needed = add.getBoundingClientRect().right - row.getBoundingClientRect().left + 12 + gridToolbarWidth;
  toolbarPlacement.value = needed <= row.clientWidth ? "grid" : "row";
}

let imagesRowObserver: ResizeObserver | null = null;
watch(imagesRowEl, (el) => {
  imagesRowObserver?.disconnect();
  imagesRowObserver = null;
  if (el) {
    imagesRowObserver = new ResizeObserver(syncToolbarPlacement);
    imagesRowObserver.observe(el);
  }
  nextTick(syncToolbarPlacement);
});
watch(() => editImages.value.length, () => nextTick(syncToolbarPlacement));
onBeforeUnmount(() => imagesRowObserver?.disconnect());

/** contenteditable 里粘贴：图片仍走原来的入库流程，其余一律降级成纯文本 */
function handleEditorPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items;
  const hasImage = !!items && Array.from(items).some((i) => i.kind === "file" && i.type.startsWith("image/"));
  if (hasImage) {
    void handlePaste(e);
    return;
  }
  // 从浏览器/Word 带来的标签会被序列化层按自己的规则还原，悄悄改写正文；只收纯文本
  e.preventDefault();
  const text = e.clipboardData?.getData("text/plain") || "";
  if (text) document.execCommand("insertText", false, text);
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

// 折叠态缩略图地址：缩略图缓存优先，回退原图地址；无地址时返回空串（模板据此渲染占位而非空 src 的 img）
function thumbSrc(filename: string): string {
  return getThumb(props.memo.id, filename) || thumbnailUrls.value.get(filename) || "";
}

// 编辑态缩略图地址
function editThumbSrc(filename: string): string {
  return getThumb(props.memo.id, filename) || editImageUrls.value.get(filename) || "";
}

let thumbLoadSeq = 0;

async function loadThumbnailImages() {
  const seq = ++thumbLoadSeq;
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
  // 已有更新的调用发出，丢弃本次过期结果，避免覆盖新数据
  if (seq !== thumbLoadSeq) return;
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
    // 没有提醒时间时不还原旧规则：库里可能留着「时间已清空、规则还在」的半成品（移入垃圾桶会这样）
    repeatChoice.value = props.memo.remind_at ? repeatKey(props.memo.remind_repeat) : "";
    nextTick(() => computeReminderMenuPos());
  }
}

/** 铃铛的提示：卡片上的时间角标只够显示几点，完整信息（含重复规则）在这里 */
const reminderTip = computed(() => {
  if (props.memo.is_done) return "已完成的备忘录无需提醒";
  const at = props.memo.remind_at;
  if (!at) return "设置提醒";
  const rule = repeatLabel(props.memo.remind_repeat);
  return `修改提醒：${at.slice(5, 16)}${rule ? " · " + rule : ""}`;
});

/** 标题栏的提醒角标：今天只给时分，其它日期带上月日，省得一列卡片全是同一串数字 */
const remindBadge = computed(() => {
  const at = props.memo.remind_at;
  if (!at) return "";
  if (at.slice(0, 10) === toDbDate(new Date()).slice(0, 10)) return at.slice(11, 16);
  return `${at.slice(5, 7)}-${at.slice(8, 10)} ${at.slice(11, 16)}`;
});

/** 编辑态脚注的时间：updated_at 只在保存正文/颜色/图片时刷新，天然就是「上一次改完」的时刻 */
const lastEditedText = computed(() => {
  const at = props.memo.updated_at;
  if (!at) return "";
  return `${at.slice(0, 10)} ${at.slice(11, 16)}`;
});

function chooseRepeat(v: RepeatValue) {
  repeatChoice.value = v;
  // 已经有提醒时间就直接按新规则重设，不必让用户再挑一次时间
  if (props.memo.remind_at) setReminder(props.memo.id, props.memo.remind_at, v);
}

function repeatTip(v: RepeatValue, label: string): string {
  if (!v) return "只提醒一次";
  return props.memo.remind_at ? `按${label}重复这条提醒` : `先选「${label}」，再点上面的时间生效`;
}

function setQuickReminder(remindAt: string) {
  setReminder(props.memo.id, remindAt, repeatChoice.value);
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
  repeatChoice.value = "";
  showReminderMenu.value = false;
}

// —— 右键菜单动作 ——
async function copyMemoText() {
  try {
    await navigator.clipboard.writeText(props.memo.content || "");
    showToast("已复制这条便签的文本", 2000);
  } catch (e) {
    showToast("复制失败: " + String(e), 3000);
  }
}

function ctxCopy() {
  closeMenu();
  copyMemoText();
}
/** 两个二级面板互斥：同时展开会把菜单撑得比窗口还高 */
function toggleCtxBoards() {
  ctxBoards.value = !ctxBoards.value;
  if (ctxBoards.value) ctxTags.value = false;
}
function toggleCtxTags() {
  ctxTags.value = !ctxTags.value;
  if (!ctxTags.value) return;
  ctxBoards.value = false;
  newTag.value = "";
  nextTick(() => tagInputRef.value?.focus());
}

function submitTagInput() {
  if (!newTag.value.trim()) {
    closeMenu();
    return;
  }
  const tag = normalizeTag(newTag.value);
  if (!tag) {
    showToast("标签名需以中文、字母、数字或 _ 开头，可含 - 与 /，最长 30 字", 3500);
    return;
  }
  void ctxAddTag(tag);
}

/** 追加到正文末尾，与用户手打 #标签 完全等价（刻意不落 tags 字段） */
async function ctxAddTag(tag: string) {
  const prev = props.memo.content || "";
  if (hasTag(prev, tag)) {
    showToast(`这条便签已有 #${tag}`, 2500);
    return;
  }
  closeMenu();
  const id = props.memo.id;
  const base = prev.trimEnd();
  if (!(await updateMemo(id, (base ? base + " " : "") + "#" + tag))) return;
  showToast(`已添加标签 #${tag}`, 6000, undefined, {
    label: "撤销",
    onClick: () => void updateMemo(id, prev),
  });
}
function ctxTrash() {
  closeMenu();
  handleDelete();
}
function ctxArchive() {
  closeMenu();
  const id = props.memo.id;
  const preview = contentPreview(props.memo.content);
  // 归档会让卡片从主列表消失，先播完移除动画再写库，手感与删除一致
  deleting.value = true;
  setTimeout(() => {
    void setArchived(id, true).then((ok) => {
      if (!ok) return;
      showToast(`已归档：${preview}`, 8000, undefined, {
        label: "撤销",
        onClick: () => void setArchived(id, false),
      });
    });
  }, 250);
}

/** 移入 / 移出集合都是单字段改写，可逆，所以给撤销而不是二次确认 */
function moveBoard(boardId: string) {
  closeMenu();
  const id = props.memo.id;
  const prev = props.memo.board;
  if (prev === boardId) return;
  void applyBoardMove(id, boardId, prev);
}

/** @param prev 移动前所在的集合，撤销时原样写回 */
async function applyBoardMove(id: string, board: string, prev: string) {
  // 集合已删除、条目在归档或垃圾桶里时后端会拒绝，失败提示由 setMemoBoard 给
  if (!(await setMemoBoard(id, board))) return;
  showToast(
    board ? `已加入集合「${boardName(board)}」` : "已移出集合，回到普通便签",
    6000,
    undefined,
    { label: "撤销", onClick: () => void setMemoBoard(id, prev) },
  );
}

// —— Delete ————————————————————————————
function handleDelete() {
  deleting.value = true;
  // 卡片马上会从列表里移除并卸载，撤销要用的值必须先抓成局部变量，不能等闭包里再读 props
  const id = props.memo.id;
  const remindAt = props.memo.remind_at || "";
  const preview = contentPreview(props.memo.content);
  setTimeout(() => {
    void moveToTrash(id).then((ok) => {
      if (!ok) return;
      showToast(`已移入垃圾桶：${preview}`, 8000, undefined, {
        label: "撤销",
        onClick: () => undoTrash(id, remindAt),
      });
    });
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
    // 标题浮层 Teleport 到了 body 上，不算「离开卡片」
    if (!target.closest(".memo-card.editing") && !target.closest(".md-heading-menu")) {
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
// 撤销目标：拖拽开始时的完整 id 序列。一次拖拽会中途反复调 reorderMemos，
// 只有起点快照才是「回到拖之前」的依据
let dragStartIds: string[] = [];
let dragChanged = false;
/** 当前被悬停的导航按钮元素，只为摘掉高亮类而留着引用 */
let dropEl: HTMLElement | null = null;
/** 悬停落点要移去的集合 id；「全部」按钮表示退回普通便签（空串），null 表示没落在导航上 */
let dropBoard: string | null = null;

function onMouseDown(e: MouseEvent) {
  if (editing.value) return;
  if (e.button !== 0) return;
  const target = e.target as HTMLElement;
  if (!target.closest(".drag-handle")) return;
  e.preventDefault();

  dragSourceId = props.memo.id;
  dragSourceEl = cardRef.value;
  if (!dragSourceEl) return;

  dragStartIds = memos.value.map((m) => m.id);
  dragChanged = false;
  dropBoard = null;

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

    // 悬在导航按钮上时不再参与排序吸附：一次移动同时改顺序又改归属，松手时就说不清要的是哪个
    if (updateDropTarget(e)) {
      autoScrollNav(e.clientX, e.clientY);
      return;
    }
    checkSnap(e.clientY);
    checkDragOver(e.clientY);
  }
}

/** 克隆节点是 pointer-events:none，所以 elementFromPoint 能穿透它拿到底下的导航按钮 */
function updateDropTarget(e: MouseEvent): boolean {
  const hit = document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-nav]") ?? null;
  const nav = hit?.getAttribute("data-nav") ?? "";
  const boardId = hit?.getAttribute("data-board-id") ?? "";
  // 只认两类落点：集合按钮＝移入，「全部」按钮＝退回普通便签。其余导航（今/昨/归档/垃圾桶）不改归属
  const overBoard = nav === "board" && !!boardId;
  const overOrdinary = nav === "memos";
  dropBoard = overBoard ? boardId : overOrdinary ? "" : null;

  const next = dropBoard !== null ? (hit as HTMLElement) : null;
  if (next !== dropEl) {
    dropEl?.classList.remove("drop-target");
    dropEl = next;
    dropEl?.classList.add("drop-target");
  }
  return dropBoard !== null;
}

/** 集合攒满 10 个时导航区自己会滚。指针靠近上下边缘时推一把，省得先松手、滚一下、再拖一次 */
function autoScrollNav(x: number, y: number) {
  const box = document.querySelector<HTMLElement>(".nav-boards");
  if (!box || box.scrollHeight <= box.clientHeight) return;
  const r = box.getBoundingClientRect();
  if (x < r.left || x > r.right) return;
  if (y < r.top + 14) box.scrollTop -= 8;
  else if (y > r.bottom - 14) box.scrollTop += 8;
}

function checkSnap(mouseY: number) {
  const fromMemo = memos.value.find((m) => m.id === dragSourceId);
  if (!fromMemo) return;

  // 邻居只能在「列表真正渲染出来的同一置顶组」里找。用全量 memos 时，
  // 被颜色/标签/日期/集合筛选隐藏的邻居没有 DOM 节点，下面的 querySelector 会一路 continue，拖拽看起来像坏了
  const sameGroup = visibleMemos.value.filter((m) => m.is_pinned === fromMemo.is_pinned);
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

function onMouseUp() {
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);

  const changed = isDragging.value && dragChanged;
  const snapshot = dragStartIds;
  // clearDragVisuals 会把这些抹平，落点判定必须先抓成局部变量
  const id = dragSourceId;
  const board = dropBoard;
  const prev = memos.value.find((m) => m.id === id)?.board ?? null;

  clearDragVisuals();

  if (board !== null && id && prev !== null && prev !== board) {
    void applyBoardMove(id, board, prev);
    return;
  }

  if (changed && snapshot.length > 0) {
    showToast("列表顺序已调整", 8000, undefined, {
      label: "撤销",
      onClick: () => undoReorder(snapshot),
    });
  }
}

/** 拆掉拖拽留下的全部痕迹。卡片可能在拖拽途中被卸载（列表整体重拉），所以收尾必须能脱离 mouseup 单独调用 */
function clearDragVisuals() {
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
  dragChanged = false;
  dragStartIds = [];
  document.body.classList.remove("is-dragging");
  document.querySelectorAll(".memo-card.drag-over").forEach((el) => el.classList.remove("drag-over"));
  dropEl?.classList.remove("drop-target");
  dropEl = null;
  dropBoard = null;
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

  dragChanged = true;
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
  window.removeEventListener("scroll", closeOnScroll, true);
  if (menuOpen.value) closeMenu();
  // 列表整体重拉会让卡片在拖拽中途卸载：光摘监听会留下克隆节点和全局 grabbing 光标
  clearDragVisuals();
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
    @contextmenu="openContextMenu"
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
        <span
          v-if="memo.remind_at && !memo.is_done"
          class="memo-remind"
          :title="reminderTip"
        >⏰ {{ remindBadge }}</span>
        <button
          v-if="memoBoardName && boardFilter !== memo.board"
          class="memo-board"
          :data-board-color="memoBoardColor || undefined"
          :title="`集合「${memoBoardName}」，点击打开`"
          @mousedown.stop
          @click.stop="openBoard?.(memo.board)"
        >{{ memoBoardName }}</button>
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
          :title="reminderTip"
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
          :title="reminderTip"
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
      <div class="reminder-repeat-row">
        <button
          v-for="opt in REPEAT_OPTIONS"
          :key="opt.value"
          class="reminder-repeat-btn"
          :class="{ active: repeatChoice === opt.value }"
          :title="repeatTip(opt.value, opt.label)"
          @click="chooseRepeat(opt.value)"
        >{{ opt.label }}</button>
      </div>
      <button v-if="memo.remind_at" class="danger" @click="cancelReminder">取消提醒</button>
    </div>
    </Teleport>

    <!-- 右键上下文菜单 -->
    <Teleport to="body">
    <div
      v-if="menuOpen"
      ref="ctxMenuRef"
      class="memo-context-menu"
      :style="ctxStyle"
      @click.stop
      @contextmenu.prevent
    >
      <button @click="ctxCopy">复制文本</button>
      <button @click="ctxArchive" title="归档只是从主列表收起，随时能在侧栏的归档视图里找回">归档</button>
      <button
        v-if="boards.length"
        :class="{ on: ctxBoards }"
        @click="toggleCtxBoards"
        title="放进集合后不再受自动清理影响"
      >{{ ctxBoards ? '收起集合 ‹' : '加入集合 ›' }}</button>
      <template v-if="ctxBoards">
        <div class="memo-context-sep"></div>
        <button
          v-for="b in boards"
          :key="b.id"
          :class="{ on: memo.board === b.id }"
          :title="`集合「${b.name}」`"
          @click="moveBoard(b.id)"
        >{{ memo.board === b.id ? '✓ ' : '' }}{{ b.name }}</button>
        <button v-if="memo.board" class="danger" @click="moveBoard('')">移出当前集合</button>
      </template>
      <button
        :class="{ on: ctxTags }"
        @click="toggleCtxTags"
        title="标签以 #标签 的形式写在正文里"
      >{{ ctxTags ? '收起 ‹' : '添加标签 ›' }}</button>
      <template v-if="ctxTags">
        <div class="memo-context-sep"></div>
        <div class="memo-context-input-row">
          <span class="memo-context-input-prefix">#</span>
          <input
            ref="tagInputRef"
            v-model="newTag"
            class="memo-context-input"
            placeholder="回车添加"
            maxlength="31"
            @mousedown.stop
            @click.stop
            @keydown.enter.prevent="submitTagInput"
            @keydown.esc.prevent="ctxTags = false"
          />
        </div>
      </template>
      <button class="danger" @click="ctxTrash">移入垃圾桶</button>
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
              v-if="!brokenImages.has(filename) && thumbSrc(filename)"
              :src="thumbSrc(filename)"
              class="memo-thumbnail"
              :style="{ zIndex: 2 - i, marginLeft: i > 0 ? '-12px' : '0' }"
              @click.stop="openImageViewer(i)"
              @error="markImageBroken(filename)"
            />
            <div
              v-else-if="brokenImages.has(filename)"
              class="memo-thumbnail is-broken"
              :style="{ zIndex: 2 - i, marginLeft: i > 0 ? '-12px' : '0' }"
              @click.stop="openImageViewer(i)"
              title="图片加载失败"
            >!</div>
            <div v-else class="memo-thumbnail is-loading" :style="{ zIndex: 2 - i, marginLeft: i > 0 ? '-12px' : '0' }"></div>
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
          v-if="!brokenImages.has(filename) && thumbnailUrls.get(filename)"
          :src="thumbnailUrls.get(filename)"
          class="memo-expanded-img"
          :class="{ 'anim-visible': imageExpandedAnim }"
          @click.stop="openImageViewer(i)"
          style="cursor: pointer"
          @error="markImageBroken(filename)"
        />
        <div
          v-else-if="brokenImages.has(filename)"
          class="memo-expanded-img is-broken"
          :class="{ 'anim-visible': imageExpandedAnim }"
          @click.stop="openImageViewer(i)"
        >图片加载失败</div>
        <div v-else class="memo-expanded-img is-loading" :class="{ 'anim-visible': imageExpandedAnim }"></div>
      </template>
    </div>

    <!-- Edit area -->
    <template v-if="editing">
      <div
        v-if="editMode === 'wysiwyg'"
        ref="editorEl"
        class="memo-content-edit md-editor"
        contenteditable="true"
        spellcheck="false"
        data-placeholder="写点什么…"
        @keydown="handleEditKeydown"
        @paste="handleEditorPaste"
        @blur="stopEdit"
      ></div>
      <textarea
        v-else
        ref="editArea"
        class="memo-content-edit"
        v-model="editText"
        @input="autoResize"
        @keydown="handleEditKeydown"
        @paste="handlePaste"
        @blur="stopEdit"
      ></textarea>

      <!-- 一行塞不下 2×3 工具栏时，整条移到图片上方排成一行 -->
      <MarkdownToolbar
        v-if="editMode === 'wysiwyg' && toolbarPlacement === 'row'"
        :editor="editorEl"
        placement="row"
      />

      <!-- Image editing area -->
      <div class="edit-images-area" v-if="true">
        <div ref="imagesRowEl" class="edit-images-row">
          <div
            v-for="filename in editImages"
            :key="filename"
            class="edit-image-thumb"
          >
            <img
              v-if="!brokenImages.has(filename) && editThumbSrc(filename)"
              :src="editThumbSrc(filename)"
              @error="markImageBroken(filename)"
            />
            <div v-else-if="brokenImages.has(filename)" class="edit-image-broken">!</div>
            <div v-else class="edit-image-loading"></div>
            <button class="edit-image-remove" @mousedown.prevent @click.stop="handleImageRemove(filename)" title="删除图片">×</button>
          </div>
          <button class="edit-image-add" @mousedown.prevent @click.stop="triggerFileInput" title="添加图片">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
          </button>
          <MarkdownToolbar
            v-if="editMode === 'wysiwyg' && toolbarPlacement === 'grid'"
            :editor="editorEl"
            placement="grid"
          />
        </div>
      </div>
      <input
        ref="fileInputRef"
        type="file"
        accept="image/jpeg,image/png,image/webp,image/gif"
        style="display: none"
        @change="handleFileSelect"
      />
      <div class="edit-paste-hint">
        <span v-if="lastEditedText" class="edit-last-edited">最后编辑于 {{ lastEditedText }}</span>
        <span>Ctrl+V 粘贴图片</span>
      </div>
    </template>
  </div>
</template>
