<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed, inject } from "vue";
import { useSettings } from "../composables/useSettings";
import { useUpdater } from "../composables/useUpdater";
import { version as appVersion } from "../../package.json";

const { settings, saveShortcut, saveTheme, saveSkin, saveNoteShortcut } = useSettings();
const openGuide = inject<() => void>("showGuide", () => {});
const { updating, updateAvailable, updateVersion, downloadProgress, lastError, checkForUpdates, installUpdate } = useUpdater();
const checking = ref(false);
const statusMessage = ref("");
let statusTimer: ReturnType<typeof setTimeout> | null = null;

const recording = ref(false);
const recordedKeys = ref<string[]>([]);
const displayShortcut = ref("");

const recordingNote = ref(false);
const recordedNoteKeys = ref<string[]>([]);
const displayNoteShortcut = ref("");

const isDark = computed(() => settings.value.theme === "dark");

const skins = [
  { value: "default", label: "默认灰", color: "#e0e5ec", accent: "#6c63ff" },
  { value: "dark", label: "午夜蓝", color: "#20242c", accent: "#c8a563" },
  { value: "warm", label: "陶土暖", color: "#f2ede6", accent: "#b87a5a" },
  { value: "fresh", label: "清新绿", color: "#e8ebe6", accent: "#4caf50" },
  { value: "pink", label: "烟岚青", color: "#eaeef0", accent: "#6a8a8f" },
  { value: "ocean", label: "海洋蓝", color: "#e8f0f8", accent: "#2196f3" },
];

function updateDisplay() {
  displayShortcut.value = (settings.value as any).shortcut;
  displayNoteShortcut.value = settings.value.note_shortcut || "Alt+N";
}

/** 圆形揭示过渡：从点击坐标向外晕开 */
function circularReveal(event: MouseEvent, apply: () => void) {
  const x = event.clientX;
  const y = event.clientY;
  const endRadius = Math.hypot(
    Math.max(x, window.innerWidth - x),
    Math.max(y, window.innerHeight - y)
  );

  if (document.startViewTransition) {
    const transition = document.startViewTransition(apply);
    transition.ready.then(() => {
      document.documentElement.animate(
        {
          clipPath: [
            `circle(0px at ${x}px ${y}px)`,
            `circle(${endRadius}px at ${x}px ${y}px)`,
          ],
        },
        {
          duration: 500,
          easing: "ease-in-out",
          pseudoElement: "::view-transition-new(root)",
        }
      );
    });
  } else {
    // 回退方案：覆盖层 + clip-path 动画
    const overlay = document.createElement("div");
    overlay.className = "skin-transition-overlay";
    const computedStyle = getComputedStyle(document.documentElement);
    overlay.style.background = computedStyle.getPropertyValue("--neu-bg").trim();
    document.body.appendChild(overlay);
    overlay.offsetHeight; // 强制回流
    overlay.style.setProperty("--reveal-x", x + "px");
    overlay.style.setProperty("--reveal-y", y + "px");
    overlay.style.setProperty("--reveal-r", endRadius + "px");
    overlay.classList.add("animating");
    apply();
    overlay.addEventListener("animationend", () => overlay.remove());
  }
}

function toggleTheme(event: MouseEvent) {
  const newTheme = isDark.value ? "light" : "dark";
  circularReveal(event, () => saveTheme(newTheme));
}

function selectSkin(value: string, event: MouseEvent) {
  const currentSkin = settings.value.skin || "default";
  if (value === currentSkin) return;
  circularReveal(event, () => saveSkin(value));
}

onMounted(() => {
  updateDisplay();
});

async function handleCheckUpdate() {
  console.log("[UI] 点击检查更新", { checking: checking.value, updating: updating.value, updateAvailable: updateAvailable.value });
  if (checking.value || updating.value) return;
  if (statusTimer) { clearTimeout(statusTimer); statusTimer = null; }
  statusMessage.value = "";

  // 如果已有更新缓存，直接安装
  if (updateAvailable.value) {
    console.log("[UI] 直接安装");
    await installUpdate();
    return;
  }

  // 否则检查更新
  checking.value = true;
  try {
    const update = await checkForUpdates();
    checking.value = false;
    if (update) {
      await installUpdate(update);
    } else {
      statusMessage.value = lastError.value || "已是最新版本";
      statusTimer = setTimeout(() => { statusMessage.value = ""; statusTimer = null; }, 2000);
    }
  } catch (e) {
    checking.value = false;
    statusMessage.value = "检查失败";
    statusTimer = setTimeout(() => { statusMessage.value = ""; statusTimer = null; }, 2000);
  }
}

function startRecording() {
  recording.value = true;
  recordedKeys.value = [];
  recordingNote.value = false;
}

function startRecordingNote() {
  recordingNote.value = true;
  recordedNoteKeys.value = [];
  recording.value = false;
}

function handleKeyDown(e: KeyboardEvent) {
  if (!recording.value && !recordingNote.value) return;
  e.preventDefault();
  e.stopPropagation();

  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");

  let key = e.code;
  if (e.code === "Space") key = "Space";
  else if (e.code.startsWith("Key")) key = e.code.replace("Key", "");
  else if (e.code.startsWith("Digit")) key = e.code.replace("Digit", "");
  else if (e.code === "Escape") {
    recording.value = false;
    recordingNote.value = false;
    return;
  }

  const isModifier = ["Control", "Alt", "Shift", "Meta"].includes(e.key);
  if (!isModifier) {
    const combo = [...mods, key].join("+");
    if (recording.value) {
      recordedKeys.value = [...mods, key];
      saveShortcut(combo);
      displayShortcut.value = combo;
      recording.value = false;
    } else if (recordingNote.value) {
      recordedNoteKeys.value = [...mods, key];
      saveNoteShortcut(combo);
      displayNoteShortcut.value = combo;
      recordingNote.value = false;
    }
  } else {
    if (recording.value) recordedKeys.value = mods;
    if (recordingNote.value) recordedNoteKeys.value = mods;
  }
}

onMounted(() => {
  document.addEventListener("keydown", handleKeyDown, true);
});

const settingsRef = ref<HTMLElement | null>(null);
const settingsTrackRef = ref<HTMLElement | null>(null);
const settingsThumbRef = ref<HTMLElement | null>(null);

let scrollbarHideTimer: ReturnType<typeof setTimeout> | null = null;
let _settingsUpdateThumb: (() => void) | null = null;
let _settingsListEl: HTMLElement | null = null;

function setupSettingsScrollbar() {
  const list = settingsRef.value;
  const track = settingsTrackRef.value;
  const thumb = settingsThumbRef.value;
  if (!list || !track || !thumb) return;

  // 箭头函数在空值守卫之后定义，才能继承 list/track/thumb 的非空收窄（函数声明会被提升，收窄失效）
  const updateThumb = () => {
    const { scrollTop, scrollHeight, clientHeight } = list;
    if (scrollHeight <= clientHeight) {
      track.classList.remove("visible");
      return;
    }
    const ratio = clientHeight / scrollHeight;
    const thumbH = Math.max(20, clientHeight * ratio);
    const thumbTop = (scrollTop / (scrollHeight - clientHeight)) * (clientHeight - thumbH);
    thumb.style.height = thumbH + "px";
    thumb.style.top = thumbTop + "px";
    track.classList.add("visible");
    if (scrollbarHideTimer) clearTimeout(scrollbarHideTimer);
    scrollbarHideTimer = setTimeout(() => {
      track.classList.remove("visible");
    }, 800);
  };

  _settingsUpdateThumb = updateThumb;
  _settingsListEl = list;
  list.addEventListener("scroll", updateThumb);
  window.addEventListener("resize", updateThumb);
  requestAnimationFrame(updateThumb);
}

onMounted(() => {
  setupSettingsScrollbar();
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeyDown, true);
  if (scrollbarHideTimer) clearTimeout(scrollbarHideTimer);
  if (_settingsUpdateThumb) {
    _settingsListEl?.removeEventListener("scroll", _settingsUpdateThumb);
    window.removeEventListener("resize", _settingsUpdateThumb);
    _settingsUpdateThumb = null;
    _settingsListEl = null;
  }
});
</script>

<template>
  <div class="settings-view">
    <div class="settings-scroll-inner scrollbar-hide" ref="settingsRef">
    <h2>⚙ 设置</h2>

    <div class="shortcut-row">
      <div class="setting-group">
        <div class="setting-label center-label">全局快捷键</div>
        <input
          class="neu-input"
          :class="{ recording: recording }"
          :value="recording ? recordedKeys.join('+') || '请按下快捷键组合...' : displayShortcut"
          readonly
          @click="startRecording"
          placeholder="点击录制快捷键"
        />
        <div class="shortcut-hint">
          {{ recording ? '按下 Esc 取消录制' : '按下键盘组合键进行设置' }}
        </div>
      </div>

      <div class="setting-group">
        <div class="setting-label center-label">快捷便签快捷键</div>
        <input
          class="neu-input"
          :class="{ recording: recordingNote }"
          :value="recordingNote ? recordedNoteKeys.join('+') || '请按下快捷键组合...' : displayNoteShortcut"
          readonly
          @click="startRecordingNote"
          placeholder="点击录制快捷键"
        />
        <div class="shortcut-hint">
          {{ recordingNote ? '按下 Esc 取消录制' : '独立便签窗口呼出快捷键' }}
        </div>
      </div>
    </div>

    <div class="setting-group">
      <div class="setting-label">外观模式</div>
      <button class="neu-btn" @click="toggleTheme($event)" :title="isDark ? '切换到亮色模式' : '切换到暗色模式'">
        <svg v-if="!isDark" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="5"/>
          <line x1="12" y1="1" x2="12" y2="3"/>
          <line x1="12" y1="21" x2="12" y2="23"/>
          <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
          <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
          <line x1="1" y1="12" x2="3" y2="12"/>
          <line x1="21" y1="12" x2="23" y2="12"/>
          <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
          <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
        </svg>
        <svg v-else width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
        </svg>
        <span>{{ isDark ? '暗色模式' : '亮色模式' }}</span>
      </button>
    </div>

    <div class="setting-group">
      <div class="setting-label">皮肤主题</div>
      <div class="skin-grid">
        <button
          v-for="skin in skins"
          :key="skin.value"
          class="skin-item"
          :class="{ active: settings.skin === skin.value || (!settings.skin && skin.value === 'default') }"
          @click="selectSkin(skin.value, $event)"
          :title="skin.label"
        >
          <div class="skin-swatch" :style="{ background: skin.color }">
            <div class="skin-accent" :style="{ background: skin.accent }"></div>
          </div>
          <span class="skin-label">{{ skin.label }}</span>
        </button>
      </div>
    </div>

    <div class="setting-row">
      <button class="neu-btn-sm" @click="openGuide">
        <span>引导手册</span>
      </button>
      <button class="neu-btn-sm" @click="handleCheckUpdate" :disabled="updating || checking">
        <span v-if="updating">下载中 {{ Math.round(downloadProgress) }}%...</span>
        <span v-else-if="updateAvailable">新版本 v{{ updateVersion }}，点击安装</span>
        <span v-else-if="checking">检查中<span class="loading-dots"><span>.</span><span>.</span><span>.</span></span></span>
        <span v-else-if="statusMessage">{{ statusMessage }}</span>
        <span v-else>检查更新</span>
      </button>
    </div>
    </div>
    <div class="memo-scroll-track" ref="settingsTrackRef">
      <div class="memo-scroll-thumb" ref="settingsThumbRef"></div>
    </div>
    <div class="version-text">当前版本 v{{ appVersion }}</div>
  </div>
</template>

<style scoped>
.settings-view {
  position: relative;
  overflow: hidden;
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 0;
}
.settings-scroll-inner {
  flex: 1;
  overflow-y: auto;
  padding: 20px 16px;
}

.neu-input {
  width: 100%;
  height: 40px;
  padding: 0 12px;
  background: var(--neu-bg, #e0e5ec);
  border: none;
  border-radius: 10px;
  color: var(--text-primary);
  font-size: 14px;
  text-align: center;
  cursor: pointer;
  box-shadow: 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              -3px -3px 6px var(--neu-shadow-light, #ffffff);
  outline: none;
  transition: box-shadow 0.2s;
}

.neu-input.recording {
  box-shadow: inset 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              inset -3px -3px 6px var(--neu-shadow-light, #ffffff);
}

.neu-btn {
  width: 100%;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: var(--neu-bg, #e0e5ec);
  border: none;
  border-radius: 10px;
  color: var(--text-primary);
  font-size: 14px;
  cursor: pointer;
  box-shadow: 4px 4px 8px var(--neu-shadow-dark, #b8bec7),
              -4px -4px 8px var(--neu-shadow-light, #ffffff);
  transition: box-shadow 0.2s;
}

.neu-btn:active {
  box-shadow: inset 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              inset -3px -3px 6px var(--neu-shadow-light, #ffffff);
}

.update-hint {
  margin-top: 8px;
  text-align: center;
  color: var(--text-muted, #999);
  font-size: 12px;
}

.loading-dots span {
  animation: blink 1.4s infinite both;
}
.loading-dots span:nth-child(2) { animation-delay: 0.2s; }
.loading-dots span:nth-child(3) { animation-delay: 0.4s; }

@keyframes blink {
  0%, 80%, 100% { opacity: 0; }
  40% { opacity: 1; }
}

.version-text {
  position: fixed;
  bottom: 6px;
  right: 10px;
  color: var(--text-muted, #999);
  font-size: 11px;
  pointer-events: none;
  z-index: 10;
}

.setting-row {
  display: flex;
  gap: 10px;
}

.shortcut-row {
  display: flex;
  gap: 12px;
  margin-bottom: 20px;
}
.shortcut-row .setting-group {
  flex: 1;
  margin-bottom: 0;
}
.center-label {
  text-align: center;
}

.neu-btn-sm {
  flex: 1;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--neu-bg, #e0e5ec);
  border: none;
  border-radius: 10px;
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
  box-shadow: 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              -3px -3px 6px var(--neu-shadow-light, #ffffff);
  transition: box-shadow 0.2s;
}

.neu-btn-sm:active {
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 4px var(--neu-shadow-light, #ffffff);
}

.skin-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
}

.skin-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 12px 8px;
  background: var(--neu-bg, #e0e5ec);
  border: none;
  border-radius: 12px;
  cursor: pointer;
  box-shadow: 4px 4px 8px var(--neu-shadow-dark, #b8bec7),
              -4px -4px 8px var(--neu-shadow-light, #ffffff);
  transition: all 0.2s;
}

.skin-item:active {
  box-shadow: inset 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              inset -3px -3px 6px var(--neu-shadow-light, #ffffff);
}

.skin-item.active {
  box-shadow: inset 4px 4px 8px var(--neu-shadow-dark, #b8bec7),
              inset -4px -4px 8px var(--neu-shadow-light, #ffffff);
}

.skin-swatch {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  position: relative;
  box-shadow: 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              -2px -2px 4px var(--neu-shadow-light, #ffffff);
}

.skin-accent {
  position: absolute;
  bottom: 0;
  right: 0;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  border: 2px solid var(--neu-bg, #e0e5ec);
}

.skin-label {
  font-size: 11px;
  color: var(--text-secondary);
}
</style>
