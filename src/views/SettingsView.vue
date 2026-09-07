<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, computed, inject, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useSettings } from "../composables/useSettings";
import { useMemos, type ShowToastFn } from "../composables/useMemos";
import { useUpdater } from "../composables/useUpdater";
import { DEFAULT_TEMPLATES, type MemoTemplate } from "../utils";
import { version as appVersion } from "../../package.json";

const { settings, saveShortcut, saveTheme, saveSkin, saveNoteShortcut, saveAutoTrashDays, saveAutoStart, saveTemplates } = useSettings();
const openGuide = inject<() => void>("showGuide", () => {});
const { updating, updateAvailable, updateVersion, downloadProgress, lastError, checkForUpdates, installUpdate } = useUpdater();
const checking = ref(false);
const statusMessage = ref("");
let statusTimer: ReturnType<typeof setTimeout> | null = null;

const autoTrashOptions = [
  { value: 0, label: "不自动清理" },
  { value: 1, label: "1 天后" },
  { value: 3, label: "3 天后" },
  { value: 7, label: "7 天后" },
  { value: 15, label: "15 天后" },
  { value: 30, label: "30 天后" },
];
const autoTrashDays = computed(() => settings.value.auto_trash_days ?? 3);

// 注册表写入是同步系统调用，失败要回滚展示，期间不允许连点
const autoStartBusy = ref(false);
const autoStartOn = computed(() => settings.value.auto_start ?? false);

async function handleAutoTrashDaysChange(e: Event) {
  const select = e.target as HTMLSelectElement;
  await saveAutoTrashDays(Number(select.value));
  // 保存失败时 settings 不变，这里把下拉拉回真实值，避免显示与后端不一致
  select.value = String(settings.value.auto_trash_days ?? 3);
}

async function handleToggleAutoStart() {
  if (autoStartBusy.value) return;
  autoStartBusy.value = true;
  try {
    await saveAutoStart(!autoStartOn.value);
  } finally {
    autoStartBusy.value = false;
  }
}

// —— 数据：导出 / 合并导入 ——
const showToast = inject<ShowToastFn>("showToast", (msg: string) => console.warn(msg));
const { loadMemos, loadArchivedMemos, loadBoards } = useMemos();
interface ImportReport { inserted: number; existing: number; invalid: number; archived: number; boards: number; total: number }

// 导出要写文件、导入要整库重拉，两者都不该被连点重复触发
const dataBusy = ref(false);
const importInputRef = ref<HTMLInputElement | null>(null);

function baseName(path: string) {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(i + 1) : path;
}

async function handleExport(format: "json" | "csv") {
  if (dataBusy.value) return;
  dataBusy.value = true;
  try {
    const path = await invoke<string>("export_memos", { format });
    showToast(`已导出 ${baseName(path)}`, 10000, undefined, {
      label: "打开文件夹",
      onClick: () => {
        void revealItemInDir(path).catch((e) => showToast(String(e), 4000));
      },
    });
  } catch (e) {
    showToast(String(e), 4000);
  } finally {
    dataBusy.value = false;
  }
}

async function handleImportFile(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  // 先清空 value，否则连续选同一个文件不会再触发 change
  input.value = "";
  if (!file || dataBusy.value) return;
  dataBusy.value = true;
  try {
    const r = await invoke<ImportReport>("import_memos", { payload: await file.text() });
    await Promise.all([loadMemos(), loadArchivedMemos(), loadBoards()]);
    showToast(
      `导入完成：新增 ${r.inserted} 条${r.archived > 0 ? `（含 ${r.archived} 条归档，在归档视图查看）` : ""}，`
        + `已存在跳过 ${r.existing} 条，无效 ${r.invalid} 条${r.boards > 0 ? `，顺带新建集合 ${r.boards} 个` : ""}`,
      8000,
    );
  } catch (err) {
    showToast(String(err), 5000);
  } finally {
    dataBusy.value = false;
  }
}

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
  displayShortcut.value = settings.value.shortcut;
  displayNoteShortcut.value = settings.value.note_shortcut ?? "";
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

// ✕＝清成「未设置」：后端注销全局快捷键并落盘。
// 失败时 saveShortcut 只弹提示、不改 settings，所以显示一律按 settings 回填
async function clearMain() {
  await saveShortcut("");
  displayShortcut.value = settings.value.shortcut;
}

async function clearNote() {
  await saveNoteShortcut("");
  displayNoteShortcut.value = settings.value.note_shortcut ?? "";
}

// 检测＝试触发：跑快捷键回调完全相同的一条路径，能调起窗口就说明动作链是通的
const testing = ref(false);

async function testNoteShortcut() {
  if (testing.value) return;
  testing.value = true;
  try {
    await invoke("open_quick_note");
    showToast("快捷记录小窗已调起");
  } catch (e) {
    showToast(`调起失败：${e}`);
  } finally {
    testing.value = false;
  }
}

async function testMainShortcut() {
  if (testing.value) return;
  testing.value = true;
  try {
    await invoke("test_main_window_shortcut");
    // 后端 1.6s 后才把窗口调回，提示早弹会跟着窗口一起隐藏看不见
    setTimeout(() => {
      testing.value = false;
      showToast("已收起并重新调起主窗口");
    }, 1800);
  } catch (e) {
    testing.value = false;
    showToast(`检测失败：${e}`);
  }
}

async function handleKeyDown(e: KeyboardEvent) {
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
    const isNote = recordingNote.value;
    // 先退出录制态再等保存：等待期间的按键不该被当成第二次录制
    recording.value = false;
    recordingNote.value = false;
    if (isNote) {
      recordedNoteKeys.value = [...mods, key];
      await saveNoteShortcut(combo);
      // saveNoteShortcut 只在注册成功后才改 settings，失败时这里回填成真实值
      displayNoteShortcut.value = settings.value.note_shortcut ?? "";
    } else {
      recordedKeys.value = [...mods, key];
      await saveShortcut(combo);
      displayShortcut.value = settings.value.shortcut;
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

// —— 正文模板 ——————————————————————————————
// 草稿只在本页编辑期间存在，写盘走整表覆盖。
// 存失败（超出后端条数/长度上限）时保留草稿：把用户刚粘进来的长内容一并抹掉，
// 比让他照着提示删短一点再存要糟得多。
const tplDraft = ref<MemoTemplate[] | null>(null);
const tplList = computed(() => tplDraft.value ?? settings.value.templates ?? DEFAULT_TEMPLATES);
const tplOpen = ref(-1);

function editDraft(): MemoTemplate[] {
  if (!tplDraft.value) tplDraft.value = tplList.value.map((t) => ({ ...t }));
  return tplDraft.value;
}

/** 折叠着的时候只露第一行非空内容，认得出是哪套模板就够 */
function tplPreview(t: MemoTemplate): string {
  const first = t.content.split("\n").find((l) => l.trim()) || "（空）";
  return first.replace(/^#+ */, "").slice(0, 24);
}

async function persistTemplates() {
  // 名称只在写盘的副本里规范化：直接改草稿会在用户还在输入时回填 input，光标会跳
  const list = editDraft().map((t) => ({ name: t.name.trim() || "未命名", content: t.content }));
  await saveTemplates(list);
  refreshScrollbar();
}

let tplTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * 输入后延时统一落一次盘，而不是等 change：改完直接切走或关窗时 textarea 已被移除，
 * blur 和 change 都不会触发，用户刚写的模板内容就会静默丢掉。
 */
function schedulePersist() {
  if (tplTimer) clearTimeout(tplTimer);
  tplTimer = setTimeout(() => {
    tplTimer = null;
    void persistTemplates();
  }, 600);
}

function flushPersist() {
  if (!tplTimer) return;
  clearTimeout(tplTimer);
  tplTimer = null;
  void persistTemplates();
}

function toggleTemplate(i: number) {
  if (tplOpen.value === i) {
    tplOpen.value = -1;
  } else {
    tplOpen.value = i;
    // 展开即意味着要改：先复制一份草稿，v-model 才不会改到 settings 甚至 DEFAULT_TEMPLATES 本身
    editDraft();
  }
  refreshScrollbar();
}

function addTemplate() {
  editDraft().push({ name: `模板 ${tplList.value.length + 1}`, content: "" });
  tplOpen.value = tplList.value.length - 1;
  void persistTemplates();
}

function removeTemplate(i: number) {
  editDraft().splice(i, 1);
  if (tplOpen.value === i) tplOpen.value = -1;
  else if (tplOpen.value > i) tplOpen.value--;
  void persistTemplates();
}

/** 恢复默认会盖掉用户自己写的模板，所以先应用再给一次撤销，而不是弹二次确认 */
function restoreTemplates() {
  const backup = tplList.value.map((t) => ({ ...t }));
  tplDraft.value = DEFAULT_TEMPLATES.map((t) => ({ ...t }));
  tplOpen.value = -1;
  void saveTemplates(tplDraft.value).then((ok) => {
    refreshScrollbar();
    if (!ok) return;
    showToast("已恢复默认模板", 6000, undefined, {
      label: "撤销",
      onClick: () => {
        tplDraft.value = backup;
        void saveTemplates(backup);
      },
    });
  });
}

/** 展开收起、增删都会改变内容高度，自绘滚动条的 thumb 要跟着重算 */
function refreshScrollbar() {
  nextTick(() => _settingsUpdateThumb?.());
}

onMounted(() => {
  setupSettingsScrollbar();
});

onBeforeUnmount(() => {
  flushPersist();
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

    <div class="shortcut-list">
      <div class="setting-group shortcut-group">
        <div class="setting-label">快捷记录快捷键</div>
        <div class="shortcut-line">
          <div
            class="shortcut-field"
            :class="{ recording: recordingNote }"
            @click="startRecordingNote"
          >
            <span class="shortcut-value" :class="{ empty: !displayNoteShortcut }">
              {{ recordingNote ? (recordedNoteKeys.join('+') || '请按下快捷键组合...') : (displayNoteShortcut || '未设置') }}
            </span>
            <span class="shortcut-record-hint">点击录制</span>
          </div>
          <button class="shortcut-btn shortcut-clear" :disabled="!displayNoteShortcut" title="清除快捷键" @click="clearNote">✕</button>
          <button class="shortcut-btn shortcut-test" :disabled="testing" title="试触发一次，调起快捷记录小窗" @click="testNoteShortcut">检测</button>
        </div>
        <div class="shortcut-hint">
          {{ recordingNote ? '按下 Esc 取消录制' : '用于打开快捷记录小窗' }}
        </div>
      </div>

      <div class="setting-group shortcut-group">
        <div class="setting-label">显示/隐藏窗口快捷键</div>
        <div class="shortcut-line">
          <div
            class="shortcut-field"
            :class="{ recording: recording }"
            @click="startRecording"
          >
            <span class="shortcut-value" :class="{ empty: !displayShortcut }">
              {{ recording ? (recordedKeys.join('+') || '请按下快捷键组合...') : (displayShortcut || '未设置') }}
            </span>
            <span class="shortcut-record-hint">点击录制</span>
          </div>
          <button class="shortcut-btn shortcut-clear" :disabled="!displayShortcut" title="清除快捷键" @click="clearMain">✕</button>
          <button class="shortcut-btn shortcut-test" :disabled="testing" title="试触发一次：收起主窗口并自动调回" @click="testMainShortcut">检测</button>
        </div>
        <div class="shortcut-hint">
          {{ recording ? '按下 Esc 取消录制' : '用于呼出/收起侧边栏主窗口' }}
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

    <div class="setting-group">
      <div class="setting-label">系统</div>
      <div class="switch-row">
        <span class="switch-text">开机自启</span>
        <button
          class="neu-switch"
          :class="{ on: autoStartOn }"
          :disabled="autoStartBusy"
          @click="handleToggleAutoStart"
          :title="autoStartOn ? '点击关闭开机自启' : '点击开启开机自启'"
        ><span class="neu-switch-knob"></span></button>
      </div>
      <div class="switch-row">
        <span class="switch-text">自动清理</span>
        <select class="neu-input neu-select" :value="autoTrashDays" @change="handleAutoTrashDaysChange">
          <option v-for="o in autoTrashOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </div>
      <div class="shortcut-hint">未置顶且超过所选天数未编辑的便签自动进垃圾桶，关闭后不再清理；侧栏集合里的便签不受此规则影响</div>
    </div>

    <div class="setting-group">
      <div class="setting-label">正文模板</div>
      <div
        v-for="(t, i) in tplList"
        :key="i"
        class="tpl-row"
        :class="{ open: tplOpen === i }"
      >
        <div class="tpl-head" @click="toggleTemplate(i)">
          <span class="tpl-caret">{{ tplOpen === i ? '▾' : '▸' }}</span>
          <span class="tpl-title">{{ t.name }}</span>
          <span v-if="tplOpen !== i" class="tpl-preview">{{ tplPreview(t) }}</span>
          <button class="tpl-del" title="删除这个模板" @click.stop="removeTemplate(i)">✕</button>
        </div>
        <div v-if="tplOpen === i" class="tpl-body">
          <input
            v-model="t.name"
            class="neu-input tpl-name"
            maxlength="16"
            placeholder="模板名"
            @input="schedulePersist"
          />
          <textarea
            v-model="t.content"
            class="neu-input tpl-content"
            rows="7"
            placeholder="模板内容"
            @input="schedulePersist"
          ></textarea>
        </div>
      </div>
      <div v-if="!tplList.length" class="shortcut-hint">还没有模板，点下面的「新增模板」建一个</div>
      <div class="setting-row">
        <button class="neu-btn-sm" @click="addTemplate">＋ 新增模板</button>
        <button class="neu-btn-sm" @click="restoreTemplates">恢复默认</button>
      </div>
      <div class="shortcut-hint">内容里的 {date} 插入时换成今天、{time} 换成当前时分；行首写 - [ ] 的模板插进去后可以直接在卡片上勾选</div>
    </div>

    <div class="setting-group">
      <div class="setting-label">数据</div>
      <div class="setting-row">
        <button class="neu-btn-sm" :disabled="dataBusy" @click="handleExport('json')">导出 JSON</button>
        <button class="neu-btn-sm" :disabled="dataBusy" @click="handleExport('csv')">导出 CSV</button>
        <button class="neu-btn-sm" :disabled="dataBusy" @click="importInputRef?.click()">导入 JSON</button>
      </div>
      <input
        ref="importInputRef"
        type="file"
        accept="application/json,.json"
        style="display: none"
        @change="handleImportFile"
      />
      <div class="shortcut-hint">导出与每日自动快照存在应用数据目录，快照保留 7 天；导入按条目合并，已有内容不会被覆盖</div>
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

.shortcut-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
  margin-bottom: 20px;
}
.shortcut-group {
  margin-bottom: 0;
}
.shortcut-line {
  display: flex;
  align-items: center;
  gap: 8px;
}
.shortcut-field {
  flex: 1;
  min-width: 0;
  height: 36px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border-radius: 10px;
  background: var(--neu-bg, #e0e5ec);
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 4px var(--neu-shadow-light, #ffffff);
  cursor: pointer;
  transition: box-shadow 0.2s;
}
.shortcut-field.recording {
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 4px var(--neu-shadow-light, #ffffff),
              0 0 0 1.5px var(--accent);
}
.shortcut-value {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  text-align: left;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.shortcut-value.empty {
  color: var(--text-muted, #999);
}
.shortcut-record-hint {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--text-muted, #999);
}
.shortcut-btn {
  flex-shrink: 0;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 10px;
  background: var(--neu-bg, #e0e5ec);
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
  box-shadow: 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              -3px -3px 6px var(--neu-shadow-light, #ffffff);
  transition: box-shadow 0.2s, color 0.2s;
}
.shortcut-btn:active {
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 4px var(--neu-shadow-light, #ffffff);
}
.shortcut-btn:disabled {
  opacity: 0.55;
  cursor: default;
}
.shortcut-btn:disabled:active {
  box-shadow: 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              -3px -3px 6px var(--neu-shadow-light, #ffffff);
}
.shortcut-clear {
  width: 36px;
  font-size: 12px;
}
.shortcut-clear:not(:disabled):hover {
  color: var(--danger);
}
.shortcut-test {
  padding: 0 12px;
  color: var(--text-primary);
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

.neu-btn-sm:disabled {
  opacity: 0.6;
  cursor: default;
}

.neu-btn-sm:disabled:active {
  box-shadow: 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              -3px -3px 6px var(--neu-shadow-light, #ffffff);
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

.switch-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 10px;
}

.switch-text {
  font-size: 13px;
  color: var(--text-secondary);
}

.neu-select {
  width: auto;
  min-width: 128px;
  height: 34px;
  /* 右侧 26px 留给箭头，左侧必须等宽，否则 text-align:center 是相对内容盒居中，文字会偏左 */
  padding: 0 26px;
  font-size: 13px;
  appearance: none;
  -webkit-appearance: none;
  background-image: linear-gradient(45deg, transparent 50%, var(--text-secondary) 50%),
                    linear-gradient(135deg, var(--text-secondary) 50%, transparent 50%);
  background-position: calc(100% - 14px) 15px, calc(100% - 9px) 15px;
  background-size: 5px 5px, 5px 5px;
  background-repeat: no-repeat;
}

.neu-switch {
  position: relative;
  flex-shrink: 0;
  width: 46px;
  height: 26px;
  padding: 0;
  border: none;
  border-radius: 13px;
  background: var(--neu-bg, #e0e5ec);
  cursor: pointer;
  box-shadow: inset 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              inset -3px -3px 6px var(--neu-shadow-light, #ffffff);
  transition: box-shadow 0.2s;
}

.neu-switch:disabled {
  cursor: default;
  opacity: 0.6;
}

.neu-switch-knob {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--neu-bg, #e0e5ec);
  box-shadow: 2px 2px 4px var(--neu-shadow-dark, #b8bec7),
              -2px -2px 4px var(--neu-shadow-light, #ffffff);
  transition: transform 0.2s, background 0.2s;
}

.neu-switch.on .neu-switch-knob {
  transform: translateX(20px);
  background: var(--accent, #6c63ff);
  box-shadow: 1px 1px 3px var(--neu-shadow-dark, #b8bec7);
}

/* ── 正文模板编辑 ─────────────────────────── */
.tpl-row {
  margin-bottom: 6px;
  border-radius: 10px;
  background: var(--neu-bg, #e0e5ec);
  box-shadow: 3px 3px 6px var(--neu-shadow-dark, #b8bec7),
              -3px -3px 6px var(--neu-shadow-light, #ffffff);
}

.tpl-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 9px 10px;
  min-width: 0;
  cursor: pointer;
}

.tpl-caret {
  flex: none;
  width: 10px;
  font-size: 10px;
  color: var(--text-muted);
}

.tpl-title {
  flex: none;
  max-width: 45%;
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tpl-preview {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tpl-del {
  flex: none;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 6px;
  background: none;
  color: var(--text-muted);
  font-size: 11px;
  cursor: pointer;
}

.tpl-del:hover {
  color: var(--danger, #c42b1c);
  background: rgba(125, 125, 125, 0.14);
}

.tpl-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 0 10px 10px;
}

/* 外层卡片是凸起的，里面的输入框用内凹阴影，两层才分得开 */
.tpl-body .neu-input {
  text-align: left;
  cursor: text;
  box-shadow: inset 2px 2px 5px var(--neu-shadow-dark, #b8bec7),
              inset -2px -2px 5px var(--neu-shadow-light, #ffffff);
}

.tpl-name {
  height: 32px;
  font-size: 13px;
}

.tpl-content {
  height: auto;
  padding: 8px 10px;
  font-size: 12px;
  line-height: 1.5;
  font-family: inherit;
  resize: vertical;
  overflow-y: auto;
}
</style>
