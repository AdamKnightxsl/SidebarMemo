<script setup lang="ts">
import { ref } from "vue";
import type { ToastAction } from "../composables/useMemos";

const message = ref("");
const queue = ref<string[]>([]);
const callbackQueue = ref<((() => void) | null)[]>([]);
const index = ref(0);
// 动作按钮只挂在定时 toast 上：常驻提示会排队轮播，按钮该跟哪一条说不清
const action = ref<ToastAction | null>(null);
let timer: ReturnType<typeof setTimeout> | null = null;
let cycleTimer: ReturnType<typeof setInterval> | null = null;
const CYCLE_INTERVAL = 3000;

function show(msg: string, duration = 0, onClick?: () => void, act?: ToastAction) {
  if (duration > 0) {
    clearQueue();
    message.value = msg;
    action.value = act ?? null;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { message.value = ""; action.value = null; }, duration);
    return;
  }

  // 入队常驻提示前清除定时 toast 的残留定时器，否则新提示会被提前清空
  if (timer) { clearTimeout(timer); timer = null; }
  action.value = null;

  queue.value.push(msg);
  callbackQueue.value.push(onClick ?? null);

  if (queue.value.length === 1) {
    message.value = msg;
    index.value = 0;
  } else if (queue.value.length === 2) {
    startCycling();
  }
}

function startCycling() {
  if (cycleTimer) clearInterval(cycleTimer);
  cycleTimer = setInterval(() => {
    index.value = (index.value + 1) % queue.value.length;
    message.value = queue.value[index.value];
  }, CYCLE_INTERVAL);
}

function clearQueue() {
  if (cycleTimer) { clearInterval(cycleTimer); cycleTimer = null; }
  queue.value = [];
  callbackQueue.value = [];
  index.value = 0;
}

function handleClick() {
  const cb = callbackQueue.value[index.value];
  if (cb) cb();
  dismiss();
}

/** 先收起再执行：撤销/确认都是一次性的，别让按钮在异步回调期间还能再点一下 */
function handleAction() {
  const act = action.value;
  dismiss();
  act?.onClick();
}

function dismiss() {
  if (timer) { clearTimeout(timer); timer = null; }
  clearQueue();
  action.value = null;
  message.value = "";
}

defineExpose({ show, dismiss });
</script>

<template>
  <div v-if="message" class="toast" @click="handleClick">
    <span class="toast-text">{{ message }}</span>
    <button v-if="action" class="toast-action" @click.stop="handleAction">{{ action.label }}</button>
    <span v-if="queue.length > 1" class="toast-counter">{{ index + 1 }}/{{ queue.length }}</span>
  </div>
</template>

<style scoped>
.toast {
  position: fixed;
  bottom: 20px;
  right: 20px;
  background: var(--neu-bg, #e0e5ec);
  color: var(--danger, #c42b1c);
  padding: 12px 18px;
  border-radius: 12px;
  font-size: 13px;
  z-index: 9999;
  box-shadow: 6px 6px 12px var(--neu-shadow-dark, #b8bec7),
              -6px -6px 12px var(--neu-shadow-light, #ffffff);
  max-width: 300px;
  word-break: break-all;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
}

.toast-text {
  flex: 1;
  min-width: 0;
}

.toast-counter {
  flex-shrink: 0;
  opacity: 0.6;
  font-size: 12px;
}

.toast-action {
  flex-shrink: 0;
  padding: 4px 10px;
  border: none;
  border-radius: 8px;
  background: var(--neu-bg, #e0e5ec);
  color: var(--accent, #6c63ff);
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
  box-shadow: 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              -2px -2px 4px var(--neu-shadow-light, #ffffff);
}

.toast-action:active {
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 4px var(--neu-shadow-light, #ffffff);
}
</style>
