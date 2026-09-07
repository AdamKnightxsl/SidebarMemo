<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { usePopupPosition } from "../composables/usePopupPosition";
import { useClickOutside } from "../composables/useClickOutside";
import { normalizeEditorDom } from "../composables/mdSerialize";

/**
 * 编辑态的 markdown 工具栏。
 *
 * 只作用于父组件传进来的那个 contenteditable：格式一律走 execCommand，
 * 产出的就是 marked 认得的那几种标签（strong / u / h1-h3 / ul / ol / input[checkbox]），
 * 序列化层照着还原，不必自己拼字符串。
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
  // 工具栏按钮全部 mousedown.prevent，焦点本就留在编辑区；再 focus 一次是防键盘触发
  props.editor?.focus();
  document.execCommand(cmd, false, arg);
  tidy();
  syncSelection();
  emit("change");
}

/** execCommand 会把列表塞进 <p>，不拆掉的话下一次回车结构就崩了 */
function tidy() {
  if (props.editor) normalizeEditorDom(props.editor);
}

function pickHeading(tag: string) {
  headingOpen.value = false;
  // 同一个级别再点一次退回正文，省得还要单独做「取消标题」
  const current = document.queryCommandValue("formatBlock").toLowerCase();
  run("formatBlock", current === tag ? "p" : tag);
}

function toggleHeadingMenu() {
  headingOpen.value = !headingOpen.value;
  if (headingOpen.value) nextTick(() => updatePosition());
}

function insertTask() {
  const sel = window.getSelection();
  const raw = sel && !sel.isCollapsed ? sel.toString() : "待办";
  const text = raw.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] || c);
  props.editor?.focus();
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
