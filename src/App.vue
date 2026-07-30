<script setup lang="ts">
import { ref, nextTick, onMounted, onBeforeUnmount, provide, watch } from "vue";
import SideNav from "./components/SideNav.vue";
import MemoListView from "./views/MemoListView.vue";
import SettingsView from "./views/SettingsView.vue";
import Toast from "./components/Toast.vue";
import FirstRunGuide from "./components/FirstRunGuide.vue";
import { useMemos, type Memo } from "./composables/useMemos";
import { useSettings } from "./composables/useSettings";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { useWindowSnap } from "./composables/useWindowSnap";
import { useUpdater } from "./composables/useUpdater";

const appContainer = ref<HTMLElement | null>(null);
const toastRef = ref<InstanceType<typeof Toast> | null>(null);
const memoListRef = ref<InstanceType<typeof MemoListView> | null>(null);
const { snappedEdge, isHidden, showFromEdge, animateToTray, closeToTray } = useWindowSnap();
const { checkForUpdates, installUpdate, updating, downloadProgress, updateVersion } = useUpdater();

const showUpdateNotify = ref(false);

function showToast(msg: string, duration = 0, onClick?: () => void) {
  toastRef.value?.show(msg, duration, onClick);
}
provide("showToast", showToast);

function shakeWindow() {
  if (!appContainer.value) return;
  appContainer.value.classList.add("shake");
  setTimeout(() => {
    appContainer.value?.classList.remove("shake");
  }, 600);
}

let positionSaveTimer: ReturnType<typeof setInterval> | null = null;
let unlistenReminder: (() => void) | null = null;
let unlistenQuickNote: (() => void) | null = null;
let appUnmounted = false;

function onVisibilityChange() {
  if (document.visibilityState === "visible") {
    invoke("handle_system_wakeup").catch(() => {});
  }
}

const currentView = ref<"memos" | "today" | "yesterday" | "day_before_yesterday" | "trash" | "settings">("memos");
const { loadMemos, dateFilter } = useMemos();
const { settings, loadSettings } = useSettings();

const isAlwaysOnTop = ref(true);
const showGuide = ref(false);

provide("currentView", currentView);

function closeGuide() {
  showGuide.value = false;
  localStorage.setItem("sidebarMemo_guideShown", "1");
  document.documentElement.scrollTop = 0;
  document.body.scrollTop = 0;
}

provide("showGuide", () => { showGuide.value = true; });

watch(currentView, (v) => {
  if (v === "today") dateFilter.value = "today";
  else if (v === "yesterday") dateFilter.value = "yesterday";
  else if (v === "day_before_yesterday") dateFilter.value = "day_before_yesterday";
  else if (v === "trash") dateFilter.value = "trash";
  else dateFilter.value = "all";
});

const skinClasses = ["skin-default", "skin-dark", "skin-warm", "skin-fresh", "skin-pink", "skin-ocean"];
const skinAccents: Record<string, { accent: string; accentHover: string }> = {
  "default": { accent: "#6c63ff", accentHover: "#8a83ff" },
  "dark": { accent: "#4a9eff", accentHover: "#6bb3ff" },
  "warm": { accent: "#e87c3a", accentHover: "#d06a2a" },
  "fresh": { accent: "#4caf50", accentHover: "#43a047" },
  "pink": { accent: "#e91e63", accentHover: "#c2185b" },
  "ocean": { accent: "#2196f3", accentHover: "#1976d2" },
};
const darkAccents: Record<string, { accent: string; accentHover: string }> = {
  "default": { accent: "#8a83ff", accentHover: "#a9a3ff" },
  "dark": { accent: "#4a9eff", accentHover: "#6bb3ff" },
  "warm": { accent: "#f09050", accentHover: "#e07830" },
  "fresh": { accent: "#66bb6a", accentHover: "#57a85c" },
  "pink": { accent: "#f06292", accentHover: "#e0407a" },
  "ocean": { accent: "#42a5f5", accentHover: "#2196f3" },
};

watch(() => settings.value.theme, (t) => {
  document.documentElement.classList.toggle("dark", t === "dark");
  applyAccent();
}, { immediate: true });

watch(() => settings.value.skin, (s) => {
  const el = document.documentElement;
  skinClasses.forEach(c => el.classList.remove(c));
  if (s) el.classList.add("skin-" + s);
  else el.classList.add("skin-default");
  applyAccent();
}, { immediate: true });

function applyAccent() {
  const isDark = settings.value.theme === "dark";
  const skin = settings.value.skin || "default";
  const map = isDark ? darkAccents : skinAccents;
  const colors = map[skin] || map["default"];
  document.documentElement.style.setProperty("--accent", colors.accent);
  document.documentElement.style.setProperty("--accent-hover", colors.accentHover);
}

let audioCtx: AudioContext | null = null;
let notifyBuffer: AudioBuffer | null = null;

function getAudioContext() {
  if (!audioCtx) {
    audioCtx = new AudioContext();
  }
  return audioCtx;
}

async function loadNotifySound() {
  try {
    const resp = await fetch("/sounds/notify.wav");
    const arrayBuf = await resp.arrayBuffer();
    const ctx = getAudioContext();
    notifyBuffer = await ctx.decodeAudioData(arrayBuf);
  } catch (e) { console.error(e); }
}

function playBeep() {
  try {
    const ctx = getAudioContext();
    if (ctx.state === "suspended") {
      ctx.resume();
    }
    if (notifyBuffer) {
      const source = ctx.createBufferSource();
      source.buffer = notifyBuffer;
      source.connect(ctx.destination);
      source.start();
    }
  } catch (e) { console.error(e); }
}

async function showReminder(memo: Memo) {
  if (isHidden.value) {
    await showFromEdge(1000);
  }
  // 剥离 markdown 标记用于 toast 显示
  const plainText = memo.content.replace(/[*#_~`>\[\]!()]/g, '').trim();
  const content = plainText.length > 40 ? plainText.slice(0, 40) + "..." : plainText;
  showToast("提醒：" + content, 0, () => {
    currentView.value = "memos";
    dateFilter.value = "all";
    nextTick(() => memoListRef.value?.scrollToMemo(memo.id));
  });
  shakeWindow();
  playBeep();
  await loadMemos();
}

onMounted(async () => {
  await Promise.all([loadMemos(), loadSettings(), loadNotifySound()]);
  if (!localStorage.getItem("sidebarMemo_guideShown")) {
    showGuide.value = true;
  }
  function resumeAudio() {
    try { getAudioContext().resume(); } catch (e) { console.error(e); }
    document.removeEventListener("click", resumeAudio);
    document.removeEventListener("keydown", resumeAudio);
  }
  document.addEventListener("click", resumeAudio);
  document.addEventListener("keydown", resumeAudio);
  document.addEventListener("visibilitychange", onVisibilityChange);
  unlistenReminder = await getCurrentWindow().listen<Memo>("memo-reminder-due", async ({ payload }) => {
    if (appUnmounted) return;
    await showReminder(payload);
  });
  // 监听快捷便签保存事件 → 刷新列表
  unlistenQuickNote = await getCurrentWindow().listen<string>("quick-note-saved", async () => {
    if (appUnmounted) return;
    await loadMemos();
  });
  try {
    const s = await invoke<{ always_on_top: boolean }>("get_settings");
    isAlwaysOnTop.value = s.always_on_top;
  } catch (e) {
    showToast(String(e));
  }
  try {
    await invoke("frontend_ready");
  } catch (e) {
    showToast(String(e));
  }
  checkForUpdates().then((update) => {
    if (update) {
      showUpdateNotify.value = true;
    }
  });
  positionSaveTimer = setInterval(() => {
    invoke("save_current_position");
  }, 5000);
});

onBeforeUnmount(() => {
  appUnmounted = true;
  if (positionSaveTimer) clearInterval(positionSaveTimer);
  unlistenReminder?.();
  unlistenQuickNote?.();
  document.removeEventListener("visibilitychange", onVisibilityChange);
});

async function handleUpdateFromNotify() {
  showUpdateNotify.value = false;
  await installUpdate();
}

function dismissUpdateNotify() {
  showUpdateNotify.value = false;
}

async function minimizeWindow() {
  const animated = await animateToTray();
  if (!animated) closeToTray();
}

async function toggleAlwaysOnTop() {
  try {
    const result = await invoke<boolean>("toggle_always_on_top");
    isAlwaysOnTop.value = result;
    // 吸附期间内部始终保持置顶（保障隐藏/弹出动画零闪烁，见 useWindowSnap 的 Z 序策略）：
    // 取消置顶时 Rust 已直接拆掉窗口置顶，这里重新断言；设置值已写入后端，退出吸附时会按新设置恢复
    if (!result && snappedEdge.value) {
      await getCurrentWindow().setAlwaysOnTop(true);
    }
  } catch (e) {
    showToast(String(e));
  }
}

// ── Resize handle ──
let resizeStartX = 0;
let resizeStartY = 0;
let resizeStartW = 0;
let resizeStartH = 0;

async function onResizeMouseDown(e: MouseEvent) {
  e.preventDefault();
  e.stopPropagation();
  resizeStartX = e.clientX;
  resizeStartY = e.clientY;
  const win = getCurrentWindow();
  const size = await win.innerSize();
  resizeStartW = size.width;
  resizeStartH = size.height;
  document.addEventListener("mousemove", onResizeMouseMove);
  document.addEventListener("mouseup", onResizeMouseUp);
}

function onResizeMouseMove(e: MouseEvent) {
  const dw = e.clientX - resizeStartX;
  const dh = e.clientY - resizeStartY;
  invoke("resize_window", {
    width: resizeStartW + dw,
    height: resizeStartH + dh,
  });
}

function onResizeMouseUp() {
  document.removeEventListener("mousemove", onResizeMouseMove);
  document.removeEventListener("mouseup", onResizeMouseUp);
  invoke("save_current_position");
}

// ── Bottom edge vertical resize ──
let resizeEdgeStartY = 0;
let resizeEdgeStartH = 0;

async function onBottomResizeMouseDown(e: MouseEvent) {
  e.preventDefault();
  e.stopPropagation();
  resizeEdgeStartY = e.clientY;
  const win = getCurrentWindow();
  const size = await win.innerSize();
  resizeEdgeStartH = size.height;
  document.addEventListener("mousemove", onBottomResizeMouseMove);
  document.addEventListener("mouseup", onBottomResizeMouseUp);
}

function onBottomResizeMouseMove(e: MouseEvent) {
  const dh = e.clientY - resizeEdgeStartY;
  invoke("resize_window", {
    width: 0,
    height: resizeEdgeStartH + dh,
  });
}

function onBottomResizeMouseUp() {
  document.removeEventListener("mousemove", onBottomResizeMouseMove);
  document.removeEventListener("mouseup", onBottomResizeMouseUp);
  invoke("save_current_position");
}
</script>
<template>
  <div class="app-container" ref="appContainer">
    <SideNav :current="currentView" @change="currentView = $event" />
    <div class="main-area">
      <div id="title-bar" class="title-bar">
        <span>Sidebar Memo</span>
        <div class="title-bar-btns">
          <button class="title-bar-btn pin-btn" :class="{ active: isAlwaysOnTop }" @click="toggleAlwaysOnTop" :title="isAlwaysOnTop ? '取消置顶' : '置顶窗口'">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="transform: rotate(45deg)">
              <path d="M12 2v8"/>
              <circle cx="12" cy="5" r="3"/>
              <path d="M8 12h8"/>
              <path d="M12 12v10"/>
            </svg>
          </button>
          <button class="title-bar-btn" @click="minimizeWindow" title="最小化">
            ─
          </button>
          <button class="title-bar-btn close-btn" @click="closeToTray" title="隐藏到托盘">
            ✕
          </button>
        </div>
      </div>
      <MemoListView ref="memoListRef" v-if="currentView !== 'settings'" />
      <SettingsView v-else />
    </div>
    <div class="resize-handle" @mousedown="onResizeMouseDown"></div>
    <div class="bottom-resize-handle" @mousedown="onBottomResizeMouseDown"></div>

    <!-- 启动时检测到新版本的提示卡片 -->
    <Transition name="update-notify">
      <div v-if="showUpdateNotify" class="update-notify">
        <div class="update-notify-icon">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="7 10 12 15 17 10"/>
            <line x1="12" y1="15" x2="12" y2="3"/>
          </svg>
        </div>
        <div class="update-notify-body">
          <span class="update-notify-title">发现新版本 v{{ updateVersion }}</span>
          <span class="update-notify-sub">点击更新以获得最新功能</span>
        </div>
        <button class="update-notify-btn" @click="handleUpdateFromNotify">更新</button>
        <button class="update-notify-close" @click="dismissUpdateNotify">✕</button>
      </div>
    </Transition>

    <Toast ref="toastRef" />
    <FirstRunGuide v-if="showGuide" @close="closeGuide" />
  </div>
</template>
