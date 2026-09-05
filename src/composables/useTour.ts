import { computed, nextTick, ref, watch, type CSSProperties } from "vue";
import { tourSuspended } from "./useWindowSnap";

export type TourPhase = "idle" | "welcome" | "steps" | "final";
export type TipSide = "top" | "bottom" | "left" | "right";

export interface TourText {
  title: string;
  body: string[];
  hint?: string;
}

interface TourStep extends TourText {
  /** 高亮框锚点（显示矩形按它算，保持窄） */
  anchor: string;
  /** 文案点名的邻接控件：只并入命中矩形，不加宽高亮框 */
  hitAnchor?: string;
  /** 列表非空时改用的锚点（如首条卡片） */
  anchorWithData?: string;
  onlyFirst?: boolean;
  mode: "interactive" | "passive";
  /** 空洞内允许判定为「本步操作」的选择器 */
  allow?: string;
  /** 交互生效后的反馈文案，替换 hint 显示（不前进） */
  done?: string;
  /** 列表为空时的替代文案 */
  alt?: TourText;
}

// 聚光框比锚点大一圈 PAD，贴边锚点会被 .app-container 的 overflow:hidden 裁掉 → 测量时夹回容器内
const PAD = 6;

const STEPS: TourStep[] = [
  {
    anchor: "[data-tour='quick-input-box']",
    hitAnchor: ".template-btn",
    title: "从这里记下第一条",
    body: ["Enter 直接保存", "Shift+Enter 换行", "左下角按钮插入模板"],
    hint: "→ 点输入框，写一条真实的备忘",
    done: "✓ 模板能插、框能写，写完点「下一步」",
    mode: "interactive",
    allow: ".quick-input-wrap, .template-btn",
  },
  {
    anchor: '[data-nav="today"],[data-nav="yesterday"],[data-nav="dby"]',
    title: "按日期分区，只看在意的",
    body: ["今 / 昨 / 前 只看那一天", "最上方图标＝全部备忘", "垃圾桶保留 30 天可恢复"],
    hint: "→ 点一个试试，就停在你要看的视图",
    done: "✓ 视图已经切过去了，点「下一步」继续",
    mode: "interactive",
    allow: ".nav-btn",
  },
  {
    anchor: "[data-tour='window-controls']",
    title: "窗口控制就这三个按钮",
    body: ["图钉：窗口置顶", "─：最小化", "✕：只是收起，程序仍在后台"],
    hint: "→ 点一下图钉看置顶效果",
    done: "✓ 置顶状态已切换，再点一下可以取消",
    mode: "interactive",
    allow: ".pin-btn",
  },
  {
    anchor: "[data-tour='search-box']",
    hitAnchor: ".color-filter-btn",
    title: "搜索与颜色筛选",
    body: ["关键词全文匹配", "命中的字会高亮", "右侧按钮按颜色筛选"],
    hint: "→ 点进输入框直接开始搜",
    done: "✓ 搜索和颜色筛选都在这一排，可以随便试",
    mode: "interactive",
    allow: ".search-box, .search-box input, .color-filter-btn, .color-filter-popup",
  },
  {
    anchor: "[data-tour='memo-list']",
    anchorWithData: "[data-tour='memo-list'] .memo-card",
    onlyFirst: true,
    title: "卡片上的操作",
    body: ["拖左侧 ⋮ 调整顺序", "双击卡片进入编辑", "悬停出现 置顶·颜色·完成·提醒·删除"],
    alt: {
      title: "你的记录都会出现在这里",
      body: ["写下第一条后就会变成卡片", "拖左侧 ⋮ 调整顺序", "双击卡片进入编辑", "悬停出现 置顶·颜色·完成·提醒·删除"],
    },
    mode: "passive",
  },
  {
    anchor: '[data-nav="settings"]',
    title: "个性化都在设置里",
    body: ["6 套皮肤", "亮 / 暗模式", "自定义全局快捷键"],
    hint: "→ 点这里进设置页，引导就结束了",
    mode: "interactive",
    allow: ".nav-btn",
  },
];

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
  /** 命中矩形（墙按它切分，空洞与真实控件对齐） */
  hx: number;
  hy: number;
  hw: number;
  hh: number;
}

const px = (v: number) => `${Math.max(0, Math.round(v))}px`;

// 模块级单例：与 useMemos / useSettings 一致，组件卸载不丢状态
const phase = ref<TourPhase>("idle");
const stepIndex = ref(0);
const lit = ref(false);
const live = ref(false);
const sealed = ref(false);
const text = ref<TourText>(STEPS[0]);
const hintText = ref("");
const spotStyle = ref<CSSProperties>({});
const sealStyle = ref<CSSProperties>({});
const wallsStyle = ref<CSSProperties[]>([]);
const tipStyle = ref<CSSProperties>({});
const arrowStyle = ref<CSSProperties>({});
const tipSide = ref<TipSide>("bottom");
const rootEl = ref<HTMLElement | null>(null);
const tipEl = ref<HTMLElement | null>(null);

const progress = computed(() => `${stepIndex.value + 1} / ${STEPS.length}`);
const isLastStep = computed(() => stepIndex.value === STEPS.length - 1);
const showPrev = computed(() => stepIndex.value > 0);

let currentBox: Box | null = null;

function delay(ms: number) {
  return new Promise<void>((r) => setTimeout(r, ms));
}

function visibleEls(sel: string): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(sel))
    .filter((el) => el.getClientRects().length > 0);
}

function boxOf(root: HTMLElement, sel: string, onlyFirst?: boolean, hitSel?: string): Box | null {
  let els = visibleEls(sel);
  if (!els.length) return null;
  if (onlyFirst) els = els.slice(0, 1);
  const hitEls = hitSel ? els.concat(visibleEls(hitSel)) : els;
  const w = root.getBoundingClientRect();
  const W = root.clientWidth;
  const H = root.clientHeight;

  const union = (list: HTMLElement[]) => {
    let l = Infinity, t = Infinity, r = -Infinity, b = -Infinity;
    for (const el of list) {
      const x = el.getBoundingClientRect();
      l = Math.min(l, x.left);
      t = Math.min(t, x.top);
      r = Math.max(r, x.right);
      b = Math.max(b, x.bottom);
    }
    return { l, t, r, b };
  };

  const { l, t, r, b } = union(els);
  const h = union(hitEls);
  const bw = Math.min(W, r - l + PAD * 2);
  const bh = Math.min(H - 4, b - t + PAD * 2);
  const hw = Math.min(W, h.r - h.l);
  const hh = Math.min(H - 4, h.b - h.t);
  return {
    left: Math.max(0, Math.min(W - bw, l - w.left - PAD)),
    top: Math.max(0, Math.min(H - 4 - bh, t - w.top - PAD)),
    width: bw,
    height: bh,
    // 留白那一圈归暗区（点了照样前进），不留死区
    hw,
    hh,
    hx: Math.max(0, Math.min(W - hw, h.l - w.left)),
    hy: Math.max(0, Math.min(H - 4 - hh, h.t - w.top)),
  };
}

function applyBox(b: Box) {
  const root = rootEl.value;
  if (!root) return;
  const W = root.clientWidth;
  const H = root.clientHeight;
  spotStyle.value = { left: px(b.left), top: px(b.top), width: px(b.width), height: px(b.height) };
  wallsStyle.value = [
    { left: "0px", top: "0px", width: px(W), height: px(b.hy) },
    { left: "0px", top: px(b.hy + b.hh), width: px(W), height: px(H - b.hy - b.hh) },
    { left: "0px", top: px(b.hy), width: px(b.hx), height: px(b.hh) },
    { left: px(b.hx + b.hw), top: px(b.hy), width: px(W - b.hx - b.hw), height: px(b.hh) },
  ];
  // 命中矩形只在被动步骤封死，交互步骤留空洞让点击原生落到真实控件
  sealStyle.value = { left: px(b.hx), top: px(b.hy), width: px(b.hw), height: px(b.hh) };
}

function placeTip(b: Box) {
  const root = rootEl.value;
  const el = tipEl.value;
  if (!root || !el) return;
  const W = root.clientWidth;
  const H = root.clientHeight;
  const tw = el.offsetWidth;
  const th = el.offsetHeight;
  const gap = 12;
  const edge = 10;
  let side: TipSide;
  let top: number;
  let left: number;

  // 窄长条（侧边导航）优先左右布局，400px 宽的窗口里上下摆会挤出可视区
  const preferSide = b.width <= 60 && b.height >= 60;
  if (preferSide && W - (b.left + b.width) >= tw + edge) {
    side = "right"; left = b.left + b.width + gap; top = b.top + b.height / 2 - th / 2;
  } else if (preferSide && b.left >= tw + edge) {
    side = "left"; left = b.left - tw - gap; top = b.top + b.height / 2 - th / 2;
  } else if (b.top + b.height + gap + th < H - edge) {
    side = "bottom"; top = b.top + b.height + gap; left = b.left + b.width / 2 - tw / 2;
  } else if (b.top - gap - th >= edge) {
    side = "top"; top = b.top - gap - th; left = b.left + b.width / 2 - tw / 2;
  } else {
    side = "bottom"; top = H - th - edge; left = b.left + b.width / 2 - tw / 2;
  }

  left = Math.max(edge, Math.min(W - tw - edge, left));
  top = Math.max(edge, Math.min(H - th - edge, top));
  tipSide.value = side;
  tipStyle.value = { left: px(left), top: px(top) };

  const cx = b.left + b.width / 2;
  const cy = b.top + b.height / 2;
  arrowStyle.value = side === "top" || side === "bottom"
    ? { left: px(Math.max(16, Math.min(tw - 24, cx - left - 5))) }
    : { top: px(Math.max(12, Math.min(th - 22, cy - top - 5))) };
}

/** 当前步骤对应的锚点矩形；列表空 / 视图切走时自动换锚点 */
function measure(): { box: Box; alt: boolean } | null {
  const root = rootEl.value;
  const s = STEPS[stepIndex.value];
  if (!root || !s) return null;
  // 声明了 anchorWithData 的步骤，数据态锚点还没出现（如首次安装一条备忘都没有）
  // → 退回高亮整个容器，并换用 alt 文案
  let empty = false;
  if (s.anchorWithData) {
    const probe = document.querySelector<HTMLElement>(s.anchorWithData);
    empty = !probe || probe.offsetHeight === 0;
  }
  const sel = empty ? s.anchor : (s.anchorWithData || s.anchor);
  const b = boxOf(root, sel, s.onlyFirst, s.hitAnchor);
  return b ? { box: b, alt: empty && !!s.alt } : null;
}

async function goStep(i: number) {
  if (phase.value !== "steps") return;
  stepIndex.value = i;
  const s = STEPS[i];
  if (!s) return finish();
  const m = measure();
  if (!m) return next();  // 锚点不在当前视图里 → 跳过该步
  currentBox = m.box;
  applyBox(m.box);
  observeAnchors(s);
  text.value = (m.alt && s.alt) ? s.alt : s;
  hintText.value = text.value.hint || "";
  sealed.value = s.mode !== "interactive";
  live.value = s.mode === "interactive";
  await nextTick();       // 气泡内容写完才量得到宽高
  placeTip(m.box);
}

function remeasure() {
  if (phase.value !== "steps" || !rootEl.value) return;
  const m = measure();
  if (!m) return;
  currentBox = m.box;
  applyBox(m.box);
  placeTip(m.box);
}

// 锚点自己会长高（输入框自动扩容）、容器会被缩放，两者都要盯着才不误判
let ro: ResizeObserver | null = null;

function ensureObserver() {
  if (ro || typeof ResizeObserver === "undefined") return;
  ro = new ResizeObserver(remeasure);
}

function observeAnchors(s: TourStep) {
  if (!ro) return;
  ro.disconnect();
  const root = rootEl.value;
  if (root?.parentElement) ro.observe(root.parentElement);
  for (const sel of [s.anchor, s.anchorWithData, s.hitAnchor]) {
    if (!sel) continue;
    const els = visibleEls(sel);
    for (const el of s.onlyFirst ? els.slice(0, 1) : els) ro.observe(el);
  }
}

function stopObserver() {
  ro?.disconnect();
}

function next() {
  if (phase.value !== "steps") return;
  if (stepIndex.value + 1 >= STEPS.length) return finish();
  void goStep(stepIndex.value + 1);
}

function prev() {
  if (phase.value !== "steps" || stepIndex.value === 0) return;
  void goStep(stepIndex.value - 1);
}

/** 交互点击的反馈：认出「这一下就是本步要的操作」，只改提示文字，不前进 */
function handleDocumentClick(e: MouseEvent) {
  if (phase.value !== "steps") return;
  const t = e.target as Element | null;
  const s = STEPS[stepIndex.value];
  if (!t || !s || s.mode !== "interactive" || !s.allow) return;
  if (t.closest(".tour-root")) return;
  const el = t.closest(s.allow);
  if (!el) return;
  // 进了设置页主视图就卸载了，后面的步骤没有锚点可指 → 直接收尾
  if (el.getAttribute("data-nav") === "settings") return finish();
  hintText.value = s.done || "✓ 生效了，点「下一步」继续";
  void nextTick(() => { if (currentBox) placeTip(currentBox); });
}

function start() {
  phase.value = "welcome";
  stepIndex.value = 0;
  lit.value = false;
}

/** 欢迎卡 → 聚光引导：先让暗幕淡入，用户才看得到「界面变暗」这个动作 */
async function beginSteps() {
  phase.value = "steps";
  ensureObserver();
  await nextTick();
  if (phase.value !== "steps") return;
  lit.value = true;
  await delay(320);
  if (phase.value !== "steps") return;
  await goStep(0);
}

function finish() {
  if (phase.value === "idle" || phase.value === "final") return;
  phase.value = "final";
  lit.value = false;
  live.value = false;
  sealed.value = false;
  stopObserver();
}

function end() {
  phase.value = "idle";
  lit.value = false;
  live.value = false;
  sealed.value = false;
  stopObserver();
  localStorage.setItem("sidebarMemo_guideShown", "1");
}

// 引导期间暂停贴边自动隐藏：窗口在引导中途滑走会让聚光框和真实界面对不上
watch(phase, (v) => { tourSuspended.value = v !== "idle"; });

export function useTour() {
  return {
    phase,
    lit,
    live,
    sealed,
    text,
    hintText,
    spotStyle,
    sealStyle,
    wallsStyle,
    tipStyle,
    arrowStyle,
    tipSide,
    rootEl,
    tipEl,
    progress,
    isLastStep,
    showPrev,
    start,
    beginSteps,
    next,
    prev,
    finish,
    end,
    remeasure,
    handleDocumentClick,
  };
}
