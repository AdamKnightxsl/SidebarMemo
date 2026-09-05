<script setup lang="ts">
import { ref, nextTick, onMounted, onBeforeUnmount } from "vue";
import { usePopupPosition } from "../composables/usePopupPosition";
import { useClickOutside } from "../composables/useClickOutside";
import { isComposing } from "../utils";

const emit = defineEmits<{
  submit: [content: string];
}>();

const text = ref("");
const textarea = ref<HTMLTextAreaElement | null>(null);
const showTemplates = ref(false);
const templateBtnRef = ref<HTMLButtonElement | null>(null);
const { popupStyle: menuStyle, updatePosition: updateMenuPos } = usePopupPosition(templateBtnRef, "top-left", 8);

useClickOutside({
  ignore: [".template-menu", ".template-btn"],
  onClickOutside: () => { showTemplates.value = false; },
  eventType: "mousedown",
});

const templates = [
  { name: '待办清单', content: '## 待办\n- [ ] ' },
  { name: '订单记录', content: () => '## 订单记录\n**日期：** ' + new Date().toLocaleDateString('zh-CN') + '\n**订单号：**\n**平台：** 抖音\n**商品：**\n**金额：**\n**买家ID：**\n\n### 订单状态\n- [ ] 已发货\n- [ ] 已签收\n- [ ] 已完成\n\n### 备注\n' },
  { name: '售后记录', content: () => '## 售后记录\n**日期：** ' + new Date().toLocaleDateString('zh-CN') + '\n**订单号：**\n**商品：**\n**售后类型：** 退款/退货/换货/补偿\n\n### 问题描述\n\n### 处理方案\n\n### 处理结果\n- [ ] 已处理\n- [ ] 待跟进\n' },
  { name: '日记', content: () => '## ' + new Date().toLocaleDateString('zh-CN') + '\n\n### 今天做了\n\n### 感悟\n' },
  { name: '读书笔记', content: '## 《书名》\n\n### 核心观点\n\n### 金句\n\n### 感想\n' },
  { name: '项目计划', content: '## 项目计划\n\n### 目标\n\n### 步骤\n1. \n2. \n3. \n\n### 截止日期\n' },
];

function selectTemplate(content: string | (() => string)) {
  text.value = typeof content === 'function' ? content() : content;
  showTemplates.value = false;
  nextTick(() => { autoResize(); textarea.value?.focus(); });
}

function autoResize() {
  if (!textarea.value) return;
  if (!text.value) {
    textarea.value.style.height = "";
    textarea.value.classList.remove("has-overflow");
    return;
  }
  textarea.value.style.height = "auto";
  textarea.value.style.height = textarea.value.scrollHeight + "px";
  textarea.value.classList.toggle("has-overflow", textarea.value.scrollHeight > textarea.value.clientHeight);
}

function handleKeydown(e: KeyboardEvent) {
  // e.isComposing / keyCode 229：中文输入法组词中按回车是确认候选词，不应提交
  if (e.key === "Enter" && !e.shiftKey && !isComposing(e)) {
    e.preventDefault();
    submit();
  }
}

function submit() {
  const v = text.value.trim();
  if (v) {
    emit("submit", v);
    text.value = "";
    nextTick(() => autoResize());
  }
}

function focus() {
  textarea.value?.focus();
}

defineExpose({ focus });
</script>

<template>
  <div class="quick-input-area">
    <button class="template-btn" :class="{ open: showTemplates }" ref="templateBtnRef" @click="showTemplates = !showTemplates; if (showTemplates) updateMenuPos()" title="插入模板">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
        <polyline points="14 2 14 8 20 8"/>
        <line x1="16" y1="13" x2="8" y2="13"/>
        <line x1="16" y1="17" x2="8" y2="17"/>
        <polyline points="10 9 9 9 8 9"/>
      </svg>
    </button>
    <Teleport to="body">
      <div v-if="showTemplates" class="template-menu" :style="menuStyle" @click.stop>
      <button
        v-for="t in templates"
        :key="t.name"
        class="template-item"
        @click="selectTemplate(t.content)"
      >{{ t.name }}</button>
    </div>
    </Teleport>
    <div class="quick-input-wrap" data-tour="quick-input-box">
      <textarea
        ref="textarea"
        v-model="text"
        class="quick-input"
        placeholder="Shift+Enter 换行"
        rows="1"
        @input="autoResize"
        @keydown="handleKeydown"
      ></textarea>
      <button class="send-btn" @click="submit" :disabled="!text.trim()">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 2L11 13" />
          <path d="M22 2L15 22L11 13L2 9L22 2Z" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.quick-input-area {
  display: flex;
  position: relative;
  z-index: 10000;
}
.template-btn {
  position: absolute;
  left: 13px;
  bottom: 13px;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--neu-bg, #e0e5ec);
  color: var(--text-secondary, #888);
  will-change: transform;
  box-shadow: 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              -2px -2px 4px var(--neu-shadow-light, #ffffff);
  transition: all 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
  z-index: 1;
  flex-shrink: 0;
}
.template-btn:hover {
  color: var(--accent, #6c63ff);
}
/* 按下动效与侧栏垃圾桶按钮（.nav-btn:active）保持一致 */
.template-btn:active {
  transform: scale(0.88);
  transition: transform 0.1s ease;
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 4px var(--neu-shadow-light, #ffffff);
}
.template-btn.open {
  color: var(--accent, #6c63ff);
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 4px var(--neu-shadow-light, #ffffff);
}
.template-menu {
  position: fixed;
  transform: translateY(-100%);
  background: var(--neu-bg, #e0e5ec);
  border-radius: 10px;
  box-shadow: 4px 4px 12px rgba(0,0,0,0.15),
              -2px -2px 8px var(--neu-shadow-light, #ffffff);
  padding: 4px;
  z-index: 999999;
  min-width: 120px;
}
.template-item {
  display: block;
  width: 100%;
  padding: 8px 12px;
  border: none;
  background: none;
  text-align: left;
  cursor: pointer;
  border-radius: 6px;
  font-size: 13px;
  color: var(--text-primary, #1a1a1a);
  transition: background 0.15s;
}
.template-item:hover {
  background: none;
  box-shadow: inset 0 0 0 1px rgba(0,0,0,0.15), inset 0 2px 6px rgba(0,0,0,0.12);
}
.dark .template-item:hover {
  background: none;
  box-shadow: inset 0 0 0 1px rgba(255,255,255,0.18), inset 0 2px 6px rgba(255,255,255,0.1);
}
.quick-input-wrap {
  padding-left: 39px;
  position: relative;
  flex: 1;
  min-width: 0;
}
.send-btn {
  position: absolute;
  right: 8px;
  bottom: 8px;
  width: 28px;
  height: 28px;
  border: none;
  background: var(--neu-bg);
  color: var(--accent);
  cursor: pointer;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s, background 0.2s;
}
.send-btn:disabled {
  opacity: 0.3;
  cursor: default;
}
.send-btn:not(:disabled):hover {
  background: rgba(0, 120, 212, 0.08);
}
.send-btn svg {
  width: 16px;
  height: 16px;
}
</style>
