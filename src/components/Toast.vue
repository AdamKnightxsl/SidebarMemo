<script setup lang="ts">
import { ref } from "vue";

const message = ref("");
const queue = ref<string[]>([]);
const callbackQueue = ref<((() => void) | null)[]>([]);
const index = ref(0);
let timer: ReturnType<typeof setTimeout> | null = null;
let cycleTimer: ReturnType<typeof setInterval> | null = null;
const CYCLE_INTERVAL = 3000;

function show(msg: string, duration = 0, onClick?: () => void) {
  if (duration > 0) {
    clearQueue();
    message.value = msg;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => { message.value = ""; }, duration);
    return;
  }

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

function dismiss() {
  if (timer) { clearTimeout(timer); timer = null; }
  clearQueue();
  message.value = "";
}

defineExpose({ show, dismiss });
</script>

<template>
  <div v-if="message" class="toast" @click="handleClick">
    <span class="toast-text">{{ message }}</span>
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
</style>
