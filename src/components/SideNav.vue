<script setup lang="ts">
import { computed, inject, nextTick, ref } from "vue";
import { useMemos, type Board, type ShowToastFn } from "../composables/useMemos";
import { BOARD_COLORS } from "../composables/boardColors";
import { usePopupPosition } from "../composables/usePopupPosition";
import { useClickOutside } from "../composables/useClickOutside";
import { isComposing } from "../utils";

type NavView = "memos" | "today" | "yesterday" | "board" | "archive" | "trash" | "settings";

const props = defineProps<{
  current: string;
  currentBoard: string;
}>();

const emit = defineEmits<{
  change: [view: NavView, boardId?: string];
}>();

const showToast = inject<ShowToastFn>("showToast", () => {});
const { memos, boards, createBoard, saveBoard, deleteBoard } = useMemos();

/** 与后端 db.rs 的 BOARD_MAX 保持一致；超限后端也会拒，这里只是提前把 + 号禁用 */
const MAX_BOARDS = 10;

/** 命名弹框的锚点：新建时是 + 号，重命名时是对应的集合按钮 */
const addBtnRef = ref<HTMLButtonElement | null>(null);
const anchorEl = ref<HTMLElement | null>(null);
const nameInput = ref<HTMLInputElement | null>(null);
const { popupStyle, updatePosition } = usePopupPosition(anchorEl, "right", 6);

/** null 表示弹框关闭；id 在 rename 模式下是要改的集合，create 模式下为空 */
const naming = ref<{ mode: "create" | "rename"; id: string } | null>(null);
const draft = ref("");
/** 命名弹框里同时编辑的归属色，空串＝不着色。新建默认不着色，保持原来的皮肤强调色外观 */
const draftColor = ref("");
/** 色板默认收起：11 个色点一直摊着太乱，点当前色那颗才展开 */
const picking = ref(false);
const draftColorLabel = computed(
  () => BOARD_COLORS.find((c) => c.key === draftColor.value)?.label ?? ""
);
/** 提交中不允许再次触发（回车连按会打出两个同名集合的竞态） */
const busy = ref(false);

const menu = ref<{ board: Board; x: number; y: number; el: HTMLElement } | null>(null);

useClickOutside({
  ignore: [".board-naming", ".board-menu"],
  onClickOutside: () => {
    naming.value = null;
    menu.value = null;
  },
  eventType: "mousedown",
});

function openCreate() {
  if (boards.value.length >= MAX_BOARDS) return;
  menu.value = null;
  anchorEl.value = addBtnRef.value;
  naming.value = { mode: "create", id: "" };
  draft.value = "";
  draftColor.value = "";
  picking.value = false;
  updatePosition();
  nextTick(() => nameInput.value?.focus());
}

function renameFromMenu() {
  if (!menu.value) return;
  const { board, el } = menu.value;
  menu.value = null;
  anchorEl.value = el;
  naming.value = { mode: "rename", id: board.id };
  draft.value = board.name;
  draftColor.value = board.color;
  picking.value = false;
  updatePosition();
  nextTick(() => {
    nameInput.value?.focus();
    nameInput.value?.select();
  });
}

function onNamingKeydown(e: KeyboardEvent) {
  // 中文输入法组词中的回车是在确认候选词，不能当成提交
  if (e.key === "Escape") {
    naming.value = null;
  } else if (e.key === "Enter" && !isComposing(e)) {
    e.preventDefault();
    void submitNaming();
  }
}

async function submitNaming() {
  const n = naming.value;
  if (!n || busy.value) return;
  busy.value = true;
  try {
    if (n.mode === "create") {
      const created = await createBoard(draft.value, draftColor.value);
      // 名称为空/重名/已满时后端报错并已 toast，保留弹框让用户就地改
      if (!created) return;
      naming.value = null;
      emit("change", "board", created.id);
    } else if (await saveBoard(n.id, draft.value, draftColor.value)) {
      naming.value = null;
    }
  } finally {
    busy.value = false;
  }
}

/** 色点只改草稿，写库等 ✓/Enter：选色选一半想反悔，Esc 或点外面就能整体放弃。选完即收起色板 */
function pickColor(key: string) {
  draftColor.value = key;
  picking.value = false;
}

function openMenu(board: Board, e: MouseEvent) {
  e.preventDefault();
  naming.value = null;
  menu.value = {
    board,
    x: e.clientX,
    y: e.clientY,
    el: e.currentTarget as HTMLElement,
  };
}

/** 菜单宽 112px、两条操作约 60px 高，贴边时夹回窗口内 */
function menuStyle(m: { x: number; y: number }) {
  return {
    position: "fixed" as const,
    left: Math.max(4, Math.min(m.x, window.innerWidth - 118)) + "px",
    top: Math.max(4, Math.min(m.y, window.innerHeight - 84)) + "px",
  };
}

function deleteFromMenu() {
  if (!menu.value) return;
  const board = menu.value.board;
  menu.value = null;
  askDeleteBoard(board);
}

/** 删除会带走集合里的便签，所以走「提示 + 确认按钮」两步，而不是直接删 */
function askDeleteBoard(board: Board) {
  const count = memos.value.filter((m) => m.board === board.id).length;
  showToast(
    count > 0
      ? `删除集合「${board.name}」，其中 ${count} 条便签会进入垃圾桶`
      : `删除空集合「${board.name}」？`,
    6000,
    undefined,
    {
      label: "确认删除",
      onClick: () => void confirmDeleteBoard(board),
    },
  );
}

async function confirmDeleteBoard(board: Board) {
  const trashed = await deleteBoard(board.id);
  if (trashed < 0) return;
  // 删的正是当前打开的集合，必须切回「全部」，否则视图停在一个已经不存在的集合上
  if (props.currentBoard === board.id) emit("change", "memos");
  showToast(trashed > 0 ? `集合已删除，${trashed} 条便签已进入垃圾桶` : "集合已删除", 3000);
}

/** 36×36 的按钮最多两行、每行两字；超过 4 字只留前 4 字，完整名靠 title */
function boardLines(name: string): string[] {
  const chars = [...name].slice(0, 4);
  if (chars.length <= 2) return [chars.join("")];
  return [chars.slice(0, 2).join(""), chars.slice(2).join("")];
}
</script>

<template>
  <div class="side-nav">
    <button
      class="nav-btn"
      :class="{ active: current === 'memos' }"
      @click="$emit('change', 'memos')"
      data-nav="memos"
      title="全部备忘"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>
        <rect x="8" y="2" width="8" height="4" rx="1" ry="1"/>
        <line x1="9" y1="12" x2="15" y2="12"/>
        <line x1="9" y1="16" x2="13" y2="16"/>
      </svg>
    </button>
    <button
      class="nav-btn nav-btn-char"
      :class="{ active: current === 'today' }"
      @click="$emit('change', 'today')"
      data-nav="today"
      title="今日"
    >
      今
    </button>
    <button
      class="nav-btn nav-btn-char"
      :class="{ active: current === 'yesterday' }"
      @click="$emit('change', 'yesterday')"
      data-nav="yesterday"
      title="昨日"
    >
      昨
    </button>

    <!-- 集合区：新建的紧挨 + 号，老的往上堆；条数多时只有这一块滚 -->
    <div class="nav-boards scrollbar-hide">
      <button
        v-for="b in boards"
        :key="b.id"
        class="nav-btn nav-btn-board"
        :class="{ 'nav-btn-single': b.name.length <= 1, active: current === 'board' && currentBoard === b.id }"
        :title="b.name"
        :data-board-id="b.id"
        :data-board-color="b.color || undefined"
        data-nav="board"
        @click="$emit('change', 'board', b.id)"
        @contextmenu="openMenu(b, $event)"
      >
        <span v-for="(line, i) in boardLines(b.name)" :key="i" class="board-line">{{ line }}</span>
        <span v-if="b.name.length > 4" class="board-more">…</span>
      </button>
      <button
        ref="addBtnRef"
        class="nav-btn nav-btn-add"
        :class="{ active: naming?.mode === 'create' }"
        :disabled="boards.length >= MAX_BOARDS"
        :title="boards.length >= MAX_BOARDS ? `最多 ${MAX_BOARDS} 个集合` : '新建集合'"
        data-nav="board-add"
        @click="openCreate"
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
          <line x1="12" y1="5" x2="12" y2="19"/>
          <line x1="5" y1="12" x2="19" y2="12"/>
        </svg>
      </button>
    </div>

    <button
      class="nav-btn"
      :class="{ active: current === 'archive' }"
      @click="$emit('change', 'archive')"
      data-nav="archive"
      title="归档"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="21 8 21 21 3 21 3 8"/>
        <rect x="1" y="3" width="22" height="5"/>
        <line x1="10" y1="13" x2="14" y2="13"/>
      </svg>
    </button>
    <button
      class="nav-btn"
      :class="{ active: current === 'trash' }"
      @click="$emit('change', 'trash')"
      data-nav="trash"
      title="垃圾桶"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="3 6 5 6 21 6"/>
        <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
        <path d="M10 11v6"/>
        <path d="M14 11v6"/>
        <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
      </svg>
    </button>
    <div class="nav-spacer"></div>
    <button
      class="nav-btn"
      :class="{ active: current === 'settings' }"
      @click="$emit('change', 'settings')"
      data-nav="settings"
      title="设置"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="3"/>
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
      </svg>
    </button>

    <Teleport to="body">
      <div v-if="naming" class="board-naming" :style="popupStyle" @click.stop>
        <div class="board-naming-row">
          <input
            ref="nameInput"
            v-model="draft"
            class="board-name-input"
            maxlength="8"
            placeholder="集合名称"
            spellcheck="false"
            @keydown="onNamingKeydown"
          />
          <button
            class="board-dot board-color-btn"
            :data-board-color="draftColor"
            :title="draftColor ? `当前颜色「${draftColorLabel}」，点击更换` : '不着色，点击选色'"
            @click="picking = !picking"
          ></button>
          <button class="board-name-ok" :disabled="busy" title="完成命名（Enter）" @click="submitNaming">✓</button>
        </div>
        <div v-if="picking" class="board-colors">
          <button
            class="board-dot"
            :class="{ active: draftColor === '' }"
            data-board-color=""
            title="不着色（跟随皮肤强调色）"
            @click="pickColor('')"
          ></button>
          <button
            v-for="c in BOARD_COLORS"
            :key="c.key"
            class="board-dot"
            :class="{ active: draftColor === c.key }"
            :data-board-color="c.key"
            :title="c.label"
            @click="pickColor(c.key)"
          ></button>
        </div>
      </div>

      <div v-if="menu" class="board-menu" :style="menuStyle(menu)" @click.stop>
        <button class="board-menu-item" @click="renameFromMenu">重命名</button>
        <button class="board-menu-item danger" @click="deleteFromMenu">删除集合</button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
/* 集合区：新建的紧挨 + 号，老的往上堆。
   平时只占按钮本身的高度（+ 号就贴在昨日下方），空间被 .nav-spacer 吃满后才自己收缩滚动。
   滚动盒会裁掉阴影，所以横向放宽 16px、纵向留 6px 内边距，再用等量负 margin 收回去 ——
   按钮中心仍落在侧栏中线上，8px 间距也和上方固定按钮一致。
   负 margin 配 align-self: flex-start：居中放置是按 margin box 算的，会把边框盒推歪 8px */
.nav-boards {
  align-self: flex-start;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  flex: 0 1 auto;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  width: calc(100% + 16px);
  margin: -6px -8px;
  padding: 6px 0;
}

/* 只有集合区可以被压缩，其余按钮在窗口很矮时也必须保持 36px */
.side-nav > .nav-btn {
  flex-shrink: 0;
}

/* 与全局 .nav-btn 同权重时样式表的加载顺序会决定胜负，所以这里一律用 .nav-btn.x 复合选择器提权 */
.nav-btn.nav-btn-board {
  position: relative;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  line-height: 1.05;
  font-weight: 700;
  font-family: inherit;
  gap: 1px;
}

/* 集合色只染文字，选中态仍旧走全局那套内阴影（图标按钮的已生效态一律用阴影表达，不加描边）。
   hover 往主题文字色靠一档，而不是换成 --text-primary，归属色才不会在悬停时消失 */
.nav-btn.nav-btn-board[data-board-color] {
  color: var(--board-color);
}
.nav-btn.nav-btn-board[data-board-color]:hover {
  color: color-mix(in srgb, var(--board-color) 65%, var(--text-primary));
}

/* 卡片正拖到这个按钮上：松手就会移进该集合。已生效态一律用内阴影，这里多叠一圈同色内描边把「即将落下」区分出来 */
.side-nav .nav-btn.drop-target {
  color: var(--board-color, var(--accent));
  transform: scale(1.06);
  box-shadow: inset 0 0 0 2px var(--board-color, var(--accent)),
              inset 3px 3px 6px var(--neu-shadow-dark),
              inset -3px -3px 6px var(--neu-shadow-light);
}

/* 单字集合沿用今/昨的字号，两字以上才需要缩到 13px 排进 2×2 */
.nav-btn-board .board-line {
  font-size: 13px;
  letter-spacing: 0;
}
.nav-btn-board.nav-btn-single .board-line {
  font-size: 19px;
}

.board-more {
  position: absolute;
  top: 1px;
  right: 3px;
  font-size: 9px;
  line-height: 1;
  opacity: 0.6;
}

.nav-btn.nav-btn-add {
  flex-shrink: 0;
}
.nav-btn-add:disabled {
  opacity: 0.35;
  cursor: default;
}

.board-naming {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 178px;
  padding: 6px;
  background: var(--neu-bg);
  border-radius: 10px;
  box-shadow: 4px 4px 12px rgba(0, 0, 0, 0.15), -2px -2px 8px var(--neu-shadow-light);
  z-index: 999999;
}

.board-naming-row {
  display: flex;
  align-items: center;
  gap: 4px;
}

.board-name-input {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 6px;
  border: none;
  border-radius: 7px;
  background: var(--neu-bg);
  color: var(--text-primary);
  font-size: 12px;
  font-family: inherit;
  outline: none;
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark), inset -2px -2px 4px var(--neu-shadow-light);
}

.board-name-ok {
  width: 26px;
  height: 26px;
  flex-shrink: 0;
  border: none;
  border-radius: 7px;
  background: var(--neu-bg);
  color: var(--accent);
  font-size: 13px;
  cursor: pointer;
  box-shadow: 2px 2px 4px var(--neu-shadow-dark), -2px -2px 4px var(--neu-shadow-light);
}
.board-name-ok:disabled {
  opacity: 0.4;
  cursor: default;
}

/* 展开的色板：1 个「不着色」+ 10 色，六个一行铺开 */
.board-colors {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 4px;
}

.board-dot {
  width: 100%;
  aspect-ratio: 1;
  padding: 0;
  border: none;
  border-radius: 50%;
  cursor: pointer;
  background: var(--board-color);
  transition: transform 0.15s ease, box-shadow 0.15s ease;
}
.board-dot:hover {
  transform: scale(1.18);
}

/* 行内那颗是当前选择的预览，也是展开入口，尺寸跟 ✓ 对齐 */
.board-dot.board-color-btn {
  width: 26px;
  height: 26px;
  flex-shrink: 0;
  box-shadow: 2px 2px 4px var(--neu-shadow-dark), -2px -2px 4px var(--neu-shadow-light);
}

/* 不着色不能画成一个白圆——看上去像第 11 种颜色。用中性底加斜杠表示「无」 */
.board-dot[data-board-color=""] {
  background: linear-gradient(
    to top right,
    transparent calc(50% - 0.6px),
    var(--text-muted) calc(50% - 0.6px),
    var(--text-muted) calc(50% + 0.6px),
    transparent calc(50% + 0.6px)
  ), var(--bg-card);
}

/* 选中态用双环：内环是弹框底色、外环是主题文字色，压在任意集合色上都还分得清 */
.board-dot.active {
  box-shadow: 0 0 0 2px var(--neu-bg), 0 0 0 3px var(--text-primary);
}

.board-menu {
  display: flex;
  flex-direction: column;
  padding: 4px;
  min-width: 112px;
  background: var(--neu-bg);
  border-radius: 10px;
  box-shadow: 4px 4px 12px rgba(0, 0, 0, 0.15), -2px -2px 8px var(--neu-shadow-light);
  z-index: 999999;
}

.board-menu-item {
  border: none;
  background: none;
  text-align: left;
  padding: 7px 10px;
  border-radius: 6px;
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
  color: var(--text-primary);
}
.board-menu-item:hover {
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15), inset 0 2px 6px rgba(0, 0, 0, 0.12);
}
.board-menu-item.danger {
  color: #d0453e;
}
.dark .board-menu-item:hover {
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18), inset 0 2px 6px rgba(255, 255, 255, 0.1);
}
.dark .board-menu-item.danger {
  color: #ff6b63;
}
</style>
