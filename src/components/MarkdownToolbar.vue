<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { usePopupPosition } from "../composables/usePopupPosition";
import { useClickOutside } from "../composables/useClickOutside";
import { normalizeEditorDom, numberHeadings, releaseListsFromHeading } from "../composables/mdSerialize";

/**
 * 编辑态的 markdown 工具栏。
 *
 * 只作用于父组件传进来的那个 contenteditable：格式尽量走 execCommand，
 * 产出的就是 marked 认得的那几种标签（strong / u / h1-h3 / ul / ol / input[checkbox]），
 * 序列化层照着还原，不必自己拼字符串。
 * 三处例外是块级命令按整行生效会不合预期，改成直接拼 DOM：
 * 标题按选区拆块、标题行上的「有序/无序」写成标题文字前缀、待办落点非法时新建段落。
 */
const props = defineProps<{
  editor: HTMLElement | null;
  /** grid＝贴在图片行右侧，2 行 3 列；row＝图片多到挤不下，整条移到图片上方排一行 */
  placement?: "grid" | "row";
}>();
const emit = defineEmits<{ (e: "change"): void }>();

/** 与查看态 .markdown-body 的 h1/h2/h3 字号保持一致，菜单里直接按该字号预览 */
const HEADINGS = [
  { tag: "h1", label: "一级标题", px: 17 },
  { tag: "h2", label: "二级标题", px: 15 },
  { tag: "h3", label: "三级标题", px: 14 },
];

const headingBtn = ref<HTMLButtonElement | null>(null);
const headingOpen = ref(false);
const { popupStyle, updatePosition } = usePopupPosition(headingBtn, "top-left", 6);

useClickOutside({
  ignore: [".md-heading-menu", ".md-tool-heading"],
  onClickOutside: () => { if (headingOpen.value) headingOpen.value = false; },
});

/** 标题/加粗/下划线是「改选中的字」，没有选区就没有作用对象 */
const hasSelection = ref(false);

function syncSelection() {
  const el = props.editor;
  const sel = window.getSelection();
  if (!el || !sel || sel.rangeCount === 0) {
    hasSelection.value = false;
    return;
  }
  const range = sel.getRangeAt(0);
  hasSelection.value = !range.collapsed && el.contains(range.commonAncestorContainer);
}

onMounted(() => document.addEventListener("selectionchange", syncSelection));
onBeforeUnmount(() => {
  document.removeEventListener("selectionchange", syncSelection);
  headingOpen.value = false;
});

const needSelection = computed(() => !hasSelection.value);

function run(cmd: string, arg?: string) {
  const el = props.editor;
  if (!el) return;
  // 工具栏按钮全部 mousedown.prevent，焦点本就留在编辑区；再 focus 一次是防键盘触发
  el.focus();
  const isList = cmd === "insertOrderedList" || cmd === "insertUnorderedList";
  const marked = isList ? headingMarkerTarget(el) : null;
  if (marked) {
    // 标题行里的「有序/无序」不产生真列表：markdown 一行只能是一种块，编号写成标题文字（## 1. 阿萨德）
    toggleHeadingMarker(marked, cmd === "insertOrderedList" ? "num" : "dot");
    numberHeadings(el);
    placeCaretAtEnd(marked);
    tidy();
    syncSelection();
    emit("change");
    return;
  }
  document.execCommand(cmd, false, arg);
  // 列表命令要先放行：Chromium 在标题行上点列表会套成 <h1><ol>…</ol></h1>，
  // 不放出列表就被 normalizeEditorDom 合并回标题，按钮看着就是没反应
  if (isList) releaseListsFromHeading(el);
  tidy();
  syncSelection();
  emit("change");
}

/** 标题行首的编号或圆点 */
const HEADING_NUM = /^[0-9]{1,3}[.、]\s+/;
const HEADING_DOT = /^[·•]\s+/;

/** 选区（或光标）完整落在同一个标题行里才走编号这条路；跨块选区还是整片变真列表 */
function headingMarkerTarget(el: HTMLElement): HTMLElement | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  const a = blockOf(range.startContainer);
  if (!a || a !== blockOf(range.endContainer) || !el.contains(a)) return null;
  return /^H[1-6]$/.test(a.tagName) ? a : null;
}

function stripHeadingMarker(h: HTMLElement): void {
  const first = h.firstChild;
  if (!first || first.nodeType !== Node.TEXT_NODE) return;
  const text = first.textContent || "";
  const m = HEADING_NUM.exec(text) || HEADING_DOT.exec(text);
  if (m) first.textContent = text.slice(m[0].length);
}

/** 同类前缀再点一次是取消；换类型（有序↔无序）直接替换 */
function toggleHeadingMarker(h: HTMLElement, kind: "num" | "dot"): void {
  const first = h.firstChild;
  const text = first && first.nodeType === Node.TEXT_NODE ? first.textContent || "" : "";
  const same = kind === "num" ? HEADING_NUM.test(text) : HEADING_DOT.test(text);
  stripHeadingMarker(h);
  if (same) return;
  const mark = kind === "num" ? "1. " : "· ";
  const node = h.firstChild;
  if (node && node.nodeType === Node.TEXT_NODE) node.textContent = mark + (node.textContent || "");
  else h.insertBefore(document.createTextNode(mark), node);
}

function placeCaretAtEnd(h: HTMLElement): void {
  const sel = window.getSelection();
  if (!sel) return;
  const range = document.createRange();
  range.selectNodeContents(h);
  range.collapse(false);
  sel.removeAllRanges();
  sel.addRange(range);
}

/** execCommand 会把列表塞进 <p>，不拆掉的话下一次回车结构就崩了 */
function tidy() {
  if (props.editor) normalizeEditorDom(props.editor);
}

/** 选区锚点所在的块标签。queryCommandValue("formatBlock") 在跨块选区会返回空串，
 *  「再点一次退回正文」就失效了，还会把整个选区刷成标题，所以直接读 DOM */
function anchorBlockTag(): string {
  const el = props.editor;
  const sel = window.getSelection();
  if (!el || !sel || sel.rangeCount === 0) return "";
  const node = sel.anchorNode;
  const from = node instanceof Element ? node : node?.parentElement;
  const block = from?.closest("h1,h2,h3,h4,h5,h6,p,div");
  return block && block !== el && el.contains(block) ? block.tagName.toLowerCase() : "";
}

function pickHeading(tag: string) {
  headingOpen.value = false;
  const el = props.editor;
  const sel = window.getSelection();
  const range = el && sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
  const block = el && range ? splitTargetBlock(el, range) : null;
  if (block && el) {
    splitBlockBySelection(el, block, tag);
    tidy();
    emit("change");
    return;
  }
  // 同一个级别再点一次退回正文，省得还要单独做「取消标题」；
  // 整行只有一种「标题退回正文」的写法（一行标题没法只让中间几个字不是标题），所以不拆块
  run("formatBlock", anchorBlockTag() === tag ? "p" : tag);
}

/** 块级节点：拆块只在这三种里做 */
const SPLITTABLE = /^(P|DIV|H[1-6])$/;

function blockOf(node: Node): HTMLElement | null {
  const from = node instanceof Element ? node : node.parentElement;
  return from?.closest<HTMLElement>("p,div,h1,h2,h3,h4,h5,h6,li,blockquote,pre") ?? null;
}

function isBr(node: Node | null): boolean {
  return !!node && node.nodeType === Node.ELEMENT_NODE && (node as Element).tagName === "BR";
}

/** 只剩空白（也没有图片、复选框）的块，留着会白占一行 */
function isBlankBlock(el: HTMLElement): boolean {
  return !(el.textContent || "").trim() && !el.querySelector("img,input");
}

/**
 * 选区能不能按「只改选中那几个字」拆块。
 * 列表项、引用、代码块里不拆：markdown 表达不了「列表项中间冒出一行标题」，
 * 硬拆出来的续行会被 lazy continuation 并进上一个列表项，正文就串位了。
 */
function splitTargetBlock(el: HTMLElement, range: Range): HTMLElement | null {
  if (range.collapsed) return null;
  const start = blockOf(range.startContainer);
  // 根节点本身不能当拆块对象：正文没有块包着时 closest 会一路爬到编辑器，拆出来的块就落到编辑器外面了
  if (!start || start === el || start !== blockOf(range.endContainer) || !el.contains(start)) return null;
  if (!SPLITTABLE.test(start.tagName) || start.closest("li,ul,ol,blockquote,pre")) return null;
  // 整块都在选区里，拆出来只剩两个空段，交给 formatBlock 更稳
  const whole = document.createRange();
  whole.selectNodeContents(start);
  const coversWhole =
    range.compareBoundaryPoints(Range.START_TO_START, whole) <= 0 &&
    range.compareBoundaryPoints(Range.END_TO_END, whole) >= 0;
  return coversWhole ? null : start;
}

/** 前段保持原块标签，选中内容单独成块，后段同样保持原块标签 */
function splitBlockBySelection(el: HTMLElement, block: HTMLElement, tag: string) {
  const sel = window.getSelection();
  const live = sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
  if (!live) return;
  const range = live.cloneRange();
  el.focus();

  // 先摘选区之后的内容：新块一插进去，尾部这些节点的位置就全变了
  const tailRange = document.createRange();
  tailRange.setStart(range.endContainer, range.endOffset);
  // 结束点必须留在 block 内部：setEndAfter(block) 会让公共祖先升到上一层，
  // extractContents 于是连 block 自己克隆一份，尾部变成 <p><p>…</p></p>
  tailRange.setEnd(block, block.childNodes.length);
  const tail = tailRange.extractContents();
  const picked = range.extractContents();
  // 行尾那个换行由块边界自带，留着会多出一个空行
  while (isBr(block.lastChild)) block.lastChild?.remove();

  // 标题只能占一行：选区跨行时按换行拆成多行标题（markdown 里没有「标题内换行」）
  const pieces: HTMLElement[] = [];
  let piece = document.createElement(tag);
  pieces.push(piece);
  for (const n of Array.from(picked.childNodes)) {
    if (isBr(n)) {
      if (!isBlankBlock(piece)) {
        piece = document.createElement(tag);
        pieces.push(piece);
      }
      continue;
    }
    piece.appendChild(n);
  }
  while (pieces.length > 1 && isBlankBlock(pieces[pieces.length - 1])) pieces.pop();

  const after = document.createElement(block.tagName.toLowerCase());
  for (const n of Array.from(tail.childNodes)) {
    if (isBr(n) && !after.firstChild) continue;
    after.appendChild(n);
  }
  const nodes: Node[] = pieces.slice();
  if (!isBlankBlock(after)) nodes.push(after);
  block.after(...nodes);
  if (isBlankBlock(block)) block.remove();

  // 新块保持选中：接着点加粗、下划线还是作用在这几个字上
  if (sel) {
    const keep = document.createRange();
    keep.setStartBefore(pieces[0]);
    keep.setEndAfter(pieces[pieces.length - 1]);
    sel.removeAllRanges();
    sel.addRange(keep);
  }
}

function toggleHeadingMenu() {
  headingOpen.value = !headingOpen.value;
  if (headingOpen.value) nextTick(() => updatePosition());
}

/**
 * 复选框只有落在段落/无序列表项里才能被序列化保住：
 * 标题/引用/代码块里会被整个丢掉，有序列表项里会丢编号，同一段里的第二个框也会丢。
 * 这些非法落点返回它所在的块，由调用方把待办插到那个块之后——宁可挪位置也不能静默丢内容。
 */
function taskBlockToSkip(el: HTMLElement): Element | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const node = sel.anchorNode;
  const from = node instanceof Element ? node : node?.parentElement;
  if (!from || !el.contains(from)) return null;
  const li = from.closest("li");
  // 空的无序列表项是合法落点；已经带框的 li 再插一个会被序列化丢掉（一处只认一个框）
  if (li && li.parentElement?.tagName === "UL" && !li.querySelector("input[type=checkbox]")) return null;
  let block: Element | null = from.closest("h1,h2,h3,h4,h5,h6,pre,blockquote");
  const list = from.closest("ul,ol");
  if (!block && list) {
    // 爬到编辑器里最外层的列表，插到整个列表之后
    block = list;
    let p = list.parentElement;
    while (p && p !== el && /^(LI|UL|OL)$/.test(p.tagName)) {
      block = p;
      p = p.parentElement;
    }
  }
  if (!block) {
    const para = from.closest("p,div");
    if (para && para !== el && el.contains(para) && para.querySelector("input[type=checkbox]")) {
      block = para;
    }
  }
  return block && el.contains(block) ? block : null;
}

function insertTask() {
  const el = props.editor;
  const sel = window.getSelection();
  // 选区可能落在编辑器外（比如另一张卡片上选的字），那不是给我们的文本
  const inEditor = !!(el && sel && sel.rangeCount > 0 && el.contains(sel.anchorNode));
  const raw = inEditor && sel && !sel.isCollapsed ? sel.toString() : "待办";
  if (!el) return;
  el.focus();
  const skip = taskBlockToSkip(el);
  if (skip) {
    // 自己拼块，不能交给 insertHTML：Chromium 会把内容提到新建的空段落外面，
    // 落到编辑器根级的裸 input + 文本不属于任何块，保存时这一行整个丢掉
    const p = document.createElement("p");
    const box = document.createElement("input");
    box.type = "checkbox";
    const t = document.createTextNode(" " + raw);
    p.appendChild(box);
    p.appendChild(t);
    skip.after(p);
    const range = document.createRange();
    range.setStartAfter(t);
    range.collapse(true);
    sel?.removeAllRanges();
    sel?.addRange(range);
    tidy();
    emit("change");
    return;
  }
  const text = raw.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] || c);
  // 复选框用真 input：编辑态能直接点勾，保存时读 checked，不再依赖「第几个框」的下标
  document.execCommand("insertHTML", false, `<input type="checkbox"> ${text}<br>`);
  tidy();
  emit("change");
}
</script>

<template>
  <div class="md-toolbar" :class="{ 'is-row': placement === 'row' }">
    <button
      ref="headingBtn"
      class="md-tool md-tool-heading"
      :disabled="needSelection"
      :class="{ open: headingOpen }"
      title="标题：选中文字后选择级别，再点一次退回正文"
      @mousedown.prevent
      @click.stop="toggleHeadingMenu"
    >标题</button>
    <button class="md-tool" :disabled="needSelection" title="加粗" @mousedown.prevent @click="run('bold')">加粗</button>
    <button class="md-tool" :disabled="needSelection" title="下划线" @mousedown.prevent @click="run('underline')">下划线</button>
    <button class="md-tool" title="无序列表" @mousedown.prevent @click="run('insertUnorderedList')">无序</button>
    <button class="md-tool" title="有序列表" @mousedown.prevent @click="run('insertOrderedList')">有序</button>
    <button class="md-tool" title="待办复选框：点一下即可打勾" @mousedown.prevent @click="insertTask">待办</button>
  </div>

  <Teleport to="body">
    <div
      v-if="headingOpen"
      class="md-heading-menu"
      :style="popupStyle"
      @click.stop
      @mousedown.prevent.stop
    >
      <button v-for="h in HEADINGS" :key="h.tag" @click="pickHeading(h.tag)">
        <span class="md-heading-sample" :style="{ fontSize: h.px + 'px' }">{{ h.label }}</span>
      </button>
    </div>
  </Teleport>
</template>
