<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useMemos, type Memo } from "../composables/useMemos";
import {
  buildMonth, dateKey, memosOn, nearestBusyMonth, barLabel,
  type Band, type MonthLayout,
} from "../composables/calendarDays";
import { isComposing, repeatKey, type RepeatValue } from "../utils";
import MemoCard from "../components/MemoCard.vue";

/**
 * 日历视图（宽版）：月历在左，选中日的清单在右。
 * 只在 currentView === 'calendar' 时挂载，窗口撑宽/还原由 App.vue 负责。
 */
const emit = defineEmits<{ jump: [id: string]; exit: [] }>();

const { visibleMemos, setReminder, addMemoAt } = useMemos();

/** 与 style.css 里 .cal-cell 的日期区高度、单条行距一一对应 */
const CELL_H = 82;
const DAY_H = 17;
const BAR_STEP = 17;
const SIDE_PAD = 5;

const WEEK = ["一", "二", "三", "四", "五", "六", "日"];
const COLOR_VAR: Record<string, string> = {
  pink: "var(--color-pink)",
  blue: "var(--color-blue)",
  green: "var(--color-green)",
  yellow: "var(--color-yellow)",
};

const boot = new Date();
const year = ref(boot.getFullYear());
const month = ref(boot.getMonth());
const todayKey = dateKey(boot.getFullYear(), boot.getMonth(), boot.getDate());
const selected = ref(todayKey);
const dir = ref<"" | "dir-next" | "dir-prev">("");

const layout = computed<MonthLayout>(() => buildMonth(year.value, month.value, visibleMemos.value));
const monthKey = computed(() => `${year.value}-${month.value}`);
const gridStyle = computed(() => ({ gridAutoRows: `${CELL_H}px` }));

const colorOf = (m: Memo) => (m.color && COLOR_VAR[m.color]) || "var(--text-muted)";
const dayList = computed<Memo[]>(() => memosOn(visibleMemos.value, selected.value));
const selDate = computed(() => new Date(+selected.value.slice(0, 4), +selected.value.slice(5, 7) - 1, +selected.value.slice(8, 10)));
const selLabel = computed(() => `${selDate.value.getMonth() + 1}月${selDate.value.getDate()}日`);
const selWeek = computed(() => WEEK[(selDate.value.getDay() + 6) % 7]);
const emptyMonth = computed(() => layout.value.total === 0);

const timeOf = (m: Memo) => (m.remind_at ? m.remind_at.slice(11, 16) : "");
const repeatText = (m: Memo) =>
  m.remind_repeat === "daily" ? "每天"
    : m.remind_repeat === "weekly" ? "每周"
      : m.remind_repeat?.startsWith("monthly:") ? `每月${+m.remind_repeat.slice(8)}日` : "";
const lanePad = (n: number) => ({ paddingTop: `${n * BAR_STEP}px` });

function shift(delta: number) {
  dir.value = delta > 0 ? "dir-next" : "dir-prev";
  const d = new Date(year.value, month.value + delta, 1);
  year.value = d.getFullYear();
  month.value = d.getMonth();
}

function goToday() {
  const d = new Date();
  const target = d.getFullYear() * 12 + d.getMonth();
  dir.value = target > year.value * 12 + month.value ? "dir-next" : "dir-prev";
  year.value = d.getFullYear();
  month.value = d.getMonth();
  selected.value = todayKey;
}

/** 点邻月补位格顺手把月份翻过去，省得先切月再点一次 */
function pickCell(key: string) {
  selected.value = key;
  const target = +key.slice(0, 4) * 100 + +key.slice(5, 7);
  const cur = year.value * 100 + (month.value + 1);
  if (target === cur) return;
  dir.value = target > cur ? "dir-next" : "dir-prev";
  year.value = +key.slice(0, 4);
  month.value = +key.slice(5, 7) - 1;
}

function jumpToBusy() {
  const t = nearestBusyMonth(visibleMemos.value, year.value, month.value);
  if (!t) return;
  dir.value = t.y * 12 + t.m > year.value * 12 + month.value ? "dir-next" : "dir-prev";
  year.value = t.y;
  month.value = t.m;
  const first = layout.value.cells.find((c) => c.inMonth && c.bars.length);
  if (first) selected.value = first.key;
}

// ── 横贯带：列宽实测，避免把像素同时写在 CSS 和 JS 里 ──────────────
const gridEl = ref<HTMLElement | null>(null);
const boxes = ref<Array<Record<string, string>>>([]);

function measureBands() {
  const grid = gridEl.value;
  const bands = layout.value.bands;
  const first = grid?.querySelector<HTMLElement>(".cal-cell");
  if (!grid || !first || !bands.length) { boxes.value = []; return; }
  // 以首个格子的实测位置为原点：网格 padding 以后怎么改都不会让带子和格子错位
  const gRect = grid.getBoundingClientRect();
  const cRect = first.getBoundingClientRect();
  const baseLeft = cRect.left - gRect.left;
  const baseTop = cRect.top - gRect.top;
  const cellW = cRect.width;
  const gap = parseFloat(getComputedStyle(grid).columnGap) || 0;
  boxes.value = bands.map((b: Band) => ({
    left: `${baseLeft + b.col * (cellW + gap) + SIDE_PAD}px`,
    top: `${baseTop + b.row * (CELL_H + gap) + DAY_H + b.lane * BAR_STEP}px`,
    width: `${b.span * cellW + (b.span - 1) * gap - SIDE_PAD * 2}px`,
  }));
}

watch(layout, () => nextTick(measureBands));
onMounted(() => nextTick(measureBands));

// ── 拖条目到别的格子＝改提醒日期 ─────────────────────────────────
const drag = ref<{ memo: Memo; x: number; y: number; to: string; moved: boolean } | null>(null);
let suppressClick = false;

function startDrag(m: Memo, e: MouseEvent) {
  if (e.button !== 0) return;
  drag.value = { memo: m, x: e.clientX, y: e.clientY, to: "", moved: false };
  document.addEventListener("mousemove", onDragMove);
  document.addEventListener("mouseup", onDragEnd);
}

function onDragMove(e: MouseEvent) {
  const d = drag.value;
  if (!d) return;
  if (!d.moved && Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) < 4) return;
  if (!d.moved) { d.moved = true; e.preventDefault(); }
  d.x = e.clientX;
  d.y = e.clientY;
  const cell = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>(".cal-cell");
  d.to = cell?.dataset.key || "";
}

async function onDragEnd() {
  document.removeEventListener("mousemove", onDragMove);
  document.removeEventListener("mouseup", onDragEnd);
  const d = drag.value;
  drag.value = null;
  if (!d?.moved) return;
  suppressClick = true;
  setTimeout(() => { suppressClick = false; }, 0);
  if (!d.to) return;
  const m = d.memo;
  if ((m.remind_at || m.created_at).slice(0, 10) === d.to) return;
  const clock = m.remind_at ? m.remind_at.slice(11) : "09:00:00";
  const repeat = (m.remind_repeat ? repeatKey(m.remind_repeat) : "") as RepeatValue;
  if (await setReminder(m.id, `${d.to} ${clock}`, repeat)) selected.value = d.to;
}

// ── 右侧详情：单击条目就地阅读，编辑铺一层 360px 浮层 ──────────────
/** 正在阅读的那条 id。空串＝还在看当天清单 */
const reading = ref("");
/** 编辑浮层。卡片本身仍由 MemoCard 管编辑态，这里只决定浮层在不在 */
const sheet = ref(false);
const sheetCard = ref<InstanceType<typeof MemoCard> | null>(null);

const detail = computed(() => dayList.value.find((m) => m.id === reading.value) ?? null);
const detailPos = computed(() => dayList.value.findIndex((m) => m.id === reading.value) + 1);

function closeDetail() {
  reading.value = "";
  sheet.value = false;
}

/**
 * 条目不再属于当前这一天（换日期、改提醒日期、归档、删除）就收回清单。
 * 换日期和条目离开走的是同一条路径，所以这里一个 watch 就够，不必在每个改 selected 的地方补。
 */
watch(detail, (m) => {
  if (!m) closeDetail();
});

function openSheet() {
  if (!detail.value || detail.value.is_done) return;
  sheet.value = true;
  nextTick(() => sheetCard.value?.startEdit());
}

function closeSheet() {
  // 卸载 contenteditable 不触发 blur，而保存挂在 blur 上，所以先让它落笔再收
  sheetCard.value?.stopEdit();
  sheet.value = false;
}

/** 编辑中被别处收掉（点完成、删除）时，editing 一变假就跟着撤浮层 */
watch(
  () => sheetCard.value?.editing,
  (on, prev) => {
    if (prev && !on) sheet.value = false;
  }
);

function onClickItem(m: Memo) {
  if (suppressClick) return;
  reading.value = m.id;
}

// ── 在选中日直接记一条 ───────────────────────────────────────────
const adding = ref(false);
const addText = ref("");
const addInput = ref<HTMLInputElement | null>(null);

function openAdd() {
  adding.value = true;
  addText.value = "";
  nextTick(() => addInput.value?.focus());
}

async function submitAdd() {
  const text = addText.value.trim();
  if (!text) { adding.value = false; return; }
  await addMemoAt(text, selected.value);
  addText.value = "";
  nextTick(() => addInput.value?.focus());
}

// ── 键盘：← → 切月，T 回今天，Esc 退出 ───────────────────────────
function onKeydown(e: KeyboardEvent) {
  if (isComposing(e)) return;
  const el = e.target as HTMLElement | null;
  if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
  if (e.key === "ArrowLeft") { e.preventDefault(); shift(-1); }
  else if (e.key === "ArrowRight") { e.preventDefault(); shift(1); }
  else if (e.key === "t" || e.key === "T") { e.preventDefault(); goToday(); }
  else if (e.key === "Escape") {
    e.preventDefault();
    if (sheet.value) closeSheet();
    else if (reading.value) closeDetail();
    else emit("exit");
  }
}

/**
 * 重测挂在网格自己的盒子上，而不是 window resize。
 * resize 回调里那套 getBoundingClientRect + getComputedStyle 是强制同步布局，窗口每帧变化都要
 * 付一次钱，正跟窗口动画抢主线程、让 WebView 跟不上窗框；观察盒子只在网格尺寸真的变了时才回调，
 * 纯位移（贴边滑动）根本不触发。
 */
let gridObserver: ResizeObserver | null = null;

onMounted(() => {
  document.addEventListener("keydown", onKeydown);
  gridObserver = new ResizeObserver(() => measureBands());
  gridObserver.observe(gridEl.value!);
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKeydown);
  gridObserver?.disconnect();
  gridObserver = null;
  document.removeEventListener("mousemove", onDragMove);
  document.removeEventListener("mouseup", onDragEnd);
});
</script>

<template>
  <div class="cal-view">
    <div class="cal-body">
      <div class="cal-left">
        <div class="cal-head">
          <button class="cal-nav" title="上个月" @click="shift(-1)">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M15 5l-7 7 7 7"/></svg>
          </button>
          <div class="cal-title">
            <span class="cal-month">{{ year }}年{{ month + 1 }}月</span>
            <span class="cal-count">本月 {{ layout.total }} 条<template v-if="layout.open"> · 未办 {{ layout.open }}</template></span>
          </div>
          <button class="cal-nav" title="下个月" @click="shift(1)">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M9 5l7 7-7 7"/></svg>
          </button>
          <button class="cal-today-btn" @click="goToday">今天</button>
        </div>

        <div class="cal-week">
          <span v-for="(w, i) in WEEK" :key="w" :class="{ we: i > 4 }">{{ w }}</span>
        </div>

        <div ref="gridEl" class="cal-grid" :class="dir" :key="monthKey" :style="gridStyle">
          <button
            v-for="c in layout.cells"
            :key="c.key"
            class="cal-cell"
            :data-key="c.key"
            :class="{ 'is-out': !c.inMonth, 'is-we': c.weekend, 'is-today': c.key === todayKey, 'is-sel': c.key === selected, 'is-drop': drag?.to === c.key }"
            @click="pickCell(c.key)"
          >
            <span class="cal-day">{{ c.day }}</span>
            <span class="cal-bars" :style="lanePad(c.laneOffset)">
              <span
                v-for="m in c.bars"
                :key="m.id"
                class="cal-bar"
                :class="{ hollow: !m.color, done: m.is_done }"
                :style="{ '--dot': colorOf(m) }"
              >{{ barLabel(m) }}</span>
              <span v-if="c.more" class="cal-more">+{{ c.more }}</span>
            </span>
          </button>

          <div
            v-for="(b, i) in layout.bands"
            :key="b.memo.id + '-' + i"
            class="cal-band"
            :class="{ hollow: !b.memo.color, done: b.memo.is_done, single: b.span === 1 }"
            :style="{ ...boxes[i], '--dot': colorOf(b.memo) }"
          >{{ b.label }}<i v-if="b.memo.remind_repeat" class="cal-band-rep">⟳</i></div>
        </div>

        <div v-if="emptyMonth" class="cal-empty-month">
          本月没有便签
          <button class="cal-jump" @click="jumpToBusy">跳到最近有便签的月份</button>
        </div>
      </div>

      <div class="cal-agenda">
        <div class="cal-agenda-head">
          <button v-if="detail" class="cal-side-btn" title="返回当天清单" @click="closeDetail">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M15 5l-7 7 7 7"/></svg>
          </button>
          <b>{{ selLabel }}</b>
          <span class="wk">{{ selWeek }}</span>
          <span class="num">{{ detail ? `第 ${detailPos} / ${dayList.length} 条` : `${dayList.length} 条` }}</span>
          <span class="spacer"></span>
          <button v-if="detail" class="cal-side-btn" title="到主界面定位这条" @click="emit('jump', detail.id)">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 4H4v5"/><path d="M15 20h5v-5"/><path d="M20 9V4h-5"/><path d="M4 15v5h5"/></svg>
          </button>
          <button v-else class="cal-add-btn" title="在这天记一条" @click="openAdd">＋</button>
        </div>

        <div v-if="adding" class="cal-add-row">
          <input
            ref="addInput"
            v-model="addText"
            class="cal-add-input"
            type="text"
            :placeholder="`${selLabel} 09:00 提醒`"
            @keydown.enter="submitAdd"
            @keydown.esc="adding = false"
            @blur="adding = false"
          />
        </div>

        <!-- 阅读态：整块给这一条，正文不折叠；右下角铅笔进编辑 -->
        <div v-if="detail" class="cal-detail">
          <MemoCard :memo="detail" detail class="cal-detail-card" @edit-request="openSheet" />
          <button
            class="cal-edit-btn"
            :disabled="detail.is_done"
            :title="detail.is_done ? '已完成的便签要先取消完成才能编辑' : '编辑这条便签'"
            @click="openSheet"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/></svg>
          </button>
        </div>

        <div v-else class="cal-agenda-list">
          <div
            v-for="(m, i) in dayList"
            :key="m.id"
            class="cal-item"
            :class="{ done: m.is_done, 'is-src': drag?.moved && drag.memo.id === m.id }"
            :style="{ '--i': String(i), '--dot': colorOf(m) }"
            @click="onClickItem(m)"
            @mousedown="startDrag(m, $event)"
          >
            <span class="cal-item-bar"></span>
            <span class="cal-item-time" :class="{ empty: !m.remind_at }">{{ timeOf(m) || '创建' }}</span>
            <span class="cal-item-text">{{ m.content }}</span>
            <span v-if="repeatText(m)" class="cal-chip rep">{{ repeatText(m) }}⟳</span>
          </div>
          <div v-if="!dayList.length" class="cal-empty">这天还没有便签<br /><span>点右上角 ＋ 记一条，或把别的条目拖到这天</span></div>
        </div>
      </div>
    </div>

    <!-- 编辑浮层：232px 那一列塞不下工具栏（会挤成两行），所以铺开 360px，
         月历不重排；卡片用回主界面那一份 MemoCard，编辑逻辑与排版完全同一套 -->
    <div v-if="sheet && detail" class="cal-edit-sheet">
      <div class="cal-agenda-head cal-sheet-head">
        <b>{{ selLabel }}</b>
        <span class="wk">{{ selWeek }}</span>
        <span class="num">第 {{ detailPos }} / {{ dayList.length }} 条</span>
        <span class="spacer"></span>
        <button class="cal-side-btn" title="保存并关闭（Esc）" @click="closeSheet">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M6 6l12 12"/><path d="M18 6L6 18"/></svg>
        </button>
      </div>
      <MemoCard ref="sheetCard" :memo="detail" class="cal-sheet-card" />
    </div>

    <Teleport to="body">
      <div v-if="drag?.moved" class="cal-drag-ghost" :style="{ left: drag.x + 'px', top: drag.y + 'px' }">
        {{ barLabel(drag.memo) }}
        <i v-if="drag.to" class="cal-drag-to">{{ drag.to.slice(5).replace("-", "/") }}</i>
      </div>
    </Teleport>
  </div>
</template>
