<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed, inject } from "vue";
import { useSettings } from "../composables/useSettings";
import { useUpdater } from "../composables/useUpdater";

const { settings, saveShortcut, saveTheme, saveSkin } = useSettings();
const openGuide = inject<() => void>("showGuide", () => {});
const { updating, updateAvailable, updateVersion, downloadProgress, checkForUpdates, installUpdate } = useUpdater();
const noUpdateMessage = ref("");

const recording = ref(false);
const recordedKeys = ref<string[]>([]);
const displayShortcut = ref("");

const isDark = computed(() => settings.value.theme === "dark");

const skins = [
  { value: "default", label: "默认灰", color: "#e0e5ec", accent: "#6c63ff" },
  { value: "dark", label: "深邃黑", color: "#2d2d2d", accent: "#4a9eff" },
  { value: "warm", label: "暖阳橙", color: "#f5f0e8", accent: "#e87c3a" },
  { value: "fresh", label: "清新绿", color: "#e8f5e8", accent: "#4caf50" },
  { value: "pink", label: "樱花粉", color: "#f8e8f0", accent: "#e91e63" },
  { value: "ocean", label: "海洋蓝", color: "#e8f0f8", accent: "#2196f3" },
];

function updateDisplay() {
  displayShortcut.value = (settings.value as any).shortcut;
}

function toggleTheme() {
  const newTheme = isDark.value ? "light" : "dark";
  saveTheme(newTheme);
}

function selectSkin(value: string) {
  saveSkin(value);
}

onMounted(() => {
  updateDisplay();
});

async function handleCheckUpdate() {
  noUpdateMessage.value = "";
  if (updateAvailable.value) {
    const update = await checkForUpdates(false);
    if (update) await installUpdate(update);
  } else {
    const update = await checkForUpdates(false);
    if (!update) {
      noUpdateMessage.value = "已是最新版本";
      setTimeout(() => { noUpdateMessage.value = ""; }, 3000);
    }
  }
}

function startRecording() {
  recording.value = true;
  recordedKeys.value = [];
}

function handleKeyDown(e: KeyboardEvent) {
  if (!recording.value) return;
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
    return;
  }

  const isModifier = ["Control", "Alt", "Shift", "Meta"].includes(e.key);
  if (!isModifier) {
    const combo = [...mods, key].join("+");
    recordedKeys.value = [...mods, key];
    saveShortcut(combo);
    displayShortcut.value = combo;
    recording.value = false;
  } else {
    recordedKeys.value = mods;
  }
}

onMounted(() => {
  document.addEventListener("keydown", handleKeyDown, true);
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeyDown, true);
});
</script>

<template>
  <div class="settings-view">
    <h2>⚙ 设置</h2>

    <div class="setting-group">
      <div class="setting-label">全局快捷键</div>
      <input
        class="neu-input"
        :class="{ recording: recording }"
        :value="recording ? recordedKeys.join('+') || '请按下快捷键组合...' : displayShortcut"
        readonly
        @click="startRecording"
        placeholder="点击录制快捷键"
      />
      <div class="shortcut-hint">
        {{ recording ? '按下 Esc 取消录制' : '点击后按下键盘组合键进行设置' }}
      </div>
    </div>

    <div class="setting-group">
      <div class="setting-label">外观模式</div>
      <button class="neu-btn" @click="toggleTheme" :title="isDark ? '切换到亮色模式' : '切换到暗色模式'">
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
          @click="selectSkin(skin.value)"
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
      <button class="neu-btn-sm" @click="handleCheckUpdate" :disabled="updating">
        <span v-if="updating">下载中 {{ Math.round(downloadProgress) }}%...</span>
        <span v-else-if="updateAvailable">新版本 v{{ updateVersion }}，点击安装</span>
        <span v-else>检查更新</span>
      </button>
    </div>
    <div v-if="noUpdateMessage" class="update-hint">{{ noUpdateMessage }}</div>
  </div>
</template>

<style scoped>
.neu-input {
  width: 100%;
  height: 40px;
  padding: 0 12px;
  background: var(--neu-bg, #e0e5ec);
  border: none;
  border-radius: 10px;
  color: var(--text-primary);
  font-size: 14px;
  cursor: pointer;
  box-shadow: inset 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              inset -3px -3px 6px var(--neu-shadow-light, #ffffff);
  outline: none;
  transition: box-shadow 0.2s;
}

.neu-input:focus {
  box-shadow: inset 4px 4px 8px var(--neu-shadow-dark, #b8bec7),
              inset -4px -4px 8px var(--neu-shadow-light, #ffffff);
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

.setting-row {
  display: flex;
  gap: 10px;
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
