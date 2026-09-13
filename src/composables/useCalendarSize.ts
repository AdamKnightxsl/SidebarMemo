// 日历视图的窗口尺寸切换。
//
// 月历要显示文字条就得比侧边栏宽，所以进日历时临时把窗口撑宽、退出时原样还回去。
// 四件事必须同时做对：
//  1) 逐帧动画只挪位置、绝不逐帧改尺寸：本窗口是 transparent:true 的分层窗口，WebView2 的合成
//     表面恒定落后窗框一两帧，逐帧变宽会让贴边那条 0.5px 边框线在窗口边缘来回闪。只挪位置则与
//     贴边滑入滑出走同一条路径，不会抖。
//  2) 尺寸那一次「瞬时」变形要挑看不见的时候做：展开时先在当前位置撑到位（多出来的宽度暂时挂在
//     屏幕外，肉眼无变化），再滑到贴边落位；收起时先滑到窄版的贴边落位（此时多出来的宽度已经在
//     屏幕外），再把尺寸收回来，收掉的那部分本来就不可见。
//  3) 落位钉住吸附边（右缘吸附 → 右边缘不动，左边缘扫出去再原路扫回来）；未吸附时以展开前的原点
//     为起点。照搬「左上角不动」会让收起变成以左边缘为轴向右缩，方向与原路相反。
//  4) 撑宽期间锁住后端的尺寸自保存（App.vue 每 5 秒存一次），否则宽版尺寸会被当成用户的常规尺寸
//     写进 settings.json，下次启动就回不去了。
import { getCurrentWindow, availableMonitors } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { snappedEdge } from "./useWindowSnap";

/** 逻辑像素。860 宽下每格约 78px，能放下 8 个字的横贯条 */
const CAL_W = 860;
const CAL_H = 620;
/** 离屏幕边缘留的缝，防止撑宽后标题栏够不着 */
const MARGIN = 8;
/** 滑动时长：匀速，不加缓动 */
const SLIDE_MS = 200;

let saved: { x: number; y: number; w: number; h: number } | null = null;
/** 进出日历都是多步异步 IPC；用户在撑宽过程中切走会让 saved 和锁错位，所以一律排队执行 */
let chain: Promise<void> = Promise.resolve();

async function applySize(logicalW: number, logicalH: number, origin?: { x: number; y: number }) {
  const win = getCurrentWindow();
  const [sf, pos, inner] = await Promise.all([
    win.scaleFactor(), win.outerPosition(), win.innerSize(),
  ]);
  let w = Math.round(logicalW * sf);
  let h = Math.round(logicalH * sf);
  let x = origin?.x ?? pos.x;
  let y = origin?.y ?? pos.y;

  const monitors = await availableMonitors().catch(() => []);
  const wa = monitors
    .map((m) => m.workArea)
    .find((a) => x >= a.position.x && x < a.position.x + a.size.width && y >= a.position.y && y < a.position.y + a.size.height)
    ?? monitors[0]?.workArea;
  if (wa) {
    w = Math.min(w, wa.size.width - MARGIN * 2);
    h = Math.min(h, wa.size.height - MARGIN * 2);
    const right = wa.position.x + wa.size.width;
    const bottom = wa.position.y + wa.size.height;
    // 钉住吸附边（见文件头第 3 条）；另一条轴沿用调用方给的起点
    const edge = snappedEdge.value;
    if (edge === "right") x = right - w;
    else if (edge === "left") x = wa.position.x;
    else if (edge === "bottom") y = bottom - h;
    else if (edge === "top") y = wa.position.y;
    // 起点之上只做一件事：把越界的部分拉回工作区内（贴着屏幕边撑宽时最常见）
    x = Math.min(Math.max(x, wa.position.x), right - w);
    y = Math.min(Math.max(y, wa.position.y), bottom - h);
  }

  // 一次原子变形（位置与尺寸同一条 SetWindowPos）；坐标语义同前端：x/y 窗口原点、w/h 客户区
  const setSize = (px: number, py: number) =>
    invoke("set_window_geometry", { x: Math.round(px), y: Math.round(py), w, h });
  const slide = () =>
    invoke("animate_window_position", {
      targetX: Math.round(x), targetY: Math.round(y), durationMs: SLIDE_MS, linear: true,
    });

  if (w > inner.width) {
    await setSize(pos.x, pos.y); // 先撑：多出来的宽度挂在屏幕外，看不见
    await slide();               // 再滑到贴边落位：逐帧只有位移，没有变形
  } else {
    await slide();               // 先滑到窄版的贴边落位：多出来的宽度已经出了屏幕
    await setSize(x, y);         // 再收：收掉的正是看不见的那部分
  }
}

/** 进日历：记下展开前的落位与尺寸 → 锁住自保存 → 撑宽 */
async function doEnter() {
  if (saved) return;
  const win = getCurrentWindow();
  const [size, pos, sf] = await Promise.all([win.innerSize(), win.outerPosition(), win.scaleFactor()]);
  saved = {
    x: pos.x, y: pos.y,
    w: Math.round(size.width / sf), h: Math.round(size.height / sf),
  };
  await invoke("set_size_save_locked", { locked: true });
  await applySize(CAL_W, CAL_H);
}

/** 出日历：还原尺寸 → 解锁 → 把还原后的尺寸存下来 */
async function doExit() {
  const back = saved;
  saved = null;
  if (back) {
    try {
      // 尺寸真的落到位之前一直保持锁定：中途解锁会让 5 秒自保存把过渡尺寸写进设置
      await applySize(
        back.w, back.h,
        // 吸附中由 applySize 按吸附边钉位（用户可能在展开期间拖动过，以当前落位为准）；
        // 没吸附时回到展开前那个原点，收起才正好沿展开那条路反向走
        snappedEdge.value ? undefined : { x: back.x, y: back.y },
      );
    } finally {
      await invoke("set_size_save_locked", { locked: false });
    }
    await invoke("save_current_position");
  } else {
    await invoke("set_size_save_locked", { locked: false });
  }
}

function enqueue(task: () => Promise<void>) {
  chain = chain.then(task).catch((e) => {
    // 单步 IPC 失败不能把整条链卡死；尺寸没换成只是难看，下一次切换会重新对齐。
    // 但也不能完全静默：命令参数写错会被 Tauri 整条拒掉，症状只是「日历不展开」，没日志根本查不到
    console.error("[calendarSize]", e);
    invoke("fe_log", { msg: `calendar size failed: ${e}` }).catch(() => {});
  });
  return chain;
}

export function enterCalendarSize() {
  return enqueue(doEnter);
}

export function exitCalendarSize() {
  return enqueue(doExit);
}
