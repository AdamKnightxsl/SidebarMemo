import { ref, onMounted, onUnmounted } from "vue";
import {
  getCurrentWindow,
  currentMonitor,
  availableMonitors,
  cursorPosition,
  PhysicalPosition,
  PhysicalSize,
} from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

export type SnapEdge = "top" | "bottom" | "left" | "right" | null;
type SnapState = "visible" | "hidden" | "showing" | "hiding";

// 以 2K 屏 (2560×1440) 为基准分辨率，固定像素按工作区尺寸的百分比换算，
// 不同分辨率下等比缩放（如 4K 横向：40/2560*3840 = 60px）
const REF_W = 2560;
const REF_H = 1440;
const SNAP_THRESHOLD = 40; // 2K 基准下的吸附阈值（物理像素）
const HIDDEN_PX = 0;       // 隐藏后完全不可见（shadow:false 后无非客户区遮挡，需完全移出屏幕）
// 注：曾尝试 HIDDEN_PX=1（不完全离屏，避免 DWM 合成集合进出）仍无法消除快速呼出/隐藏时的全屏闪烁，
// 闪烁根因不在离屏与否，而是驱动层 MPO/直接翻转模式切换，已回退为 0
const HOVER_RANGE = 5;     // 2K 基准下的悬停唤醒范围（替代原 HIDDEN_PX 的触发区域）
// 动画时长：配合 Rust 端三段二次缓动（隐藏曲线），前 80ms 走 70% 路程，中 100ms 走 70%~90%，后 140ms 走最后 10% 极慢收尾
const ANIM_DURATION = 320;
// 弹出动画时长：配合 Rust 端弹出曲线，前 80ms 走 70%，中 100ms 走 70%~90%，后 100ms 走最后 10%
const SHOW_ANIM_DURATION = 280;

// 横向/纵向按各自轴的分辨率比例换算，钳制最小 1px 防止小屏取整为 0
function scaleX(px: number, wa: { w: number }) {
  return Math.max(1, Math.round(px * wa.w / REF_W));
}
function scaleY(px: number, wa: { h: number }) {
  return Math.max(1, Math.round(px * wa.h / REF_H));
}
// 隐藏边露出像素：左右边缘按宽度比例，上下边缘按高度比例（HIDDEN_PX=0 时直接返回 0，不经过 Math.max(1,...) 钳制）
function hiddenPxFor(edge: SnapEdge, wa: { w: number; h: number }) {
  if (HIDDEN_PX === 0) return 0;
  return edge === "top" || edge === "bottom" ? scaleY(HIDDEN_PX, wa) : scaleX(HIDDEN_PX, wa);
}
// v 是否位于 [a, b] 闭区间内（不要求 a <= b）
function between(v: number, a: number, b: number) {
  return v >= Math.min(a, b) && v <= Math.max(a, b);
}

// 贴边吸附隐藏与弹出动画：拖拽窗口到屏幕边缘时自动吸附，失焦后隐藏到边缘，鼠标悬停时弹出。
export function useWindowSnap() {
  const snappedEdge = ref<SnapEdge>(null);
  const isHidden = ref(false);
  const appWindow = getCurrentWindow();

  let state: SnapState = "visible";
  let savedPosition: { x: number; y: number; w: number; h: number } | null = null;
  let unlistenFocus: (() => void) | null = null;
  let unlistenToggle: (() => void) | null = null;
  let unlistenHide: (() => void) | null = null;

  let dragStartMouse: { x: number; y: number } | null = null;
  let dragStartWin: { x: number; y: number } | null = null;
  let isDragging = false;
  let animatingToTray = false;
  let snapTarget: { x: number; y: number } | null = null;

  // 弹出动画期间收到的隐藏请求（此时 state="showing"，hideToEdge 会被状态守卫丢弃）：
  // blur = 弹出瞬间点了别的软件（完成后需复核鼠标/焦点再隐藏）；explicit = 快捷键/托盘明确要求隐藏（完成后无条件隐藏）
  let pendingHideOnShown: "blur" | "explicit" | null = null;
  // 弹出后焦点兜底轮询定时器（见 startFocusWatchdog）
  let focusWatchTimer: ReturnType<typeof setInterval> | null = null;

  // 互斥锁：保证同一时间只有一个状态转换操作在执行
  let _transitionLock = false;
  let _transitionQueue: (() => void)[] = [];

  async function withLock<T>(fn: () => Promise<T>): Promise<T | undefined> {
    if (_transitionLock) {
      // 排队等待当前操作完成
      return new Promise<T | undefined>((resolve) => {
        _transitionQueue.push(() => { resolve(withLock(fn)); });
      });
    }
    _transitionLock = true;
    try {
      return await fn();
    } finally {
      _transitionLock = false;
      // 执行排队的下一个操作
      const next = _transitionQueue.shift();
      if (next) next();
    }
  }

  // 动画通过 Rust 端执行，不受浏览器 rAF 暂停影响，帧率稳定
  // expand=true 为弹出（走 Rust 端弹出曲线，总 SHOW_ANIM_DURATION），否则为隐藏/吸附对齐（原曲线，总 ANIM_DURATION）
  async function animatePosition(targetX: number, targetY: number, _forceStartPos?: { x: number; y: number }, expand = false) {
    await invoke("animate_window_position", {
      targetX, targetY, durationMs: expand ? SHOW_ANIM_DURATION : ANIM_DURATION, expand,
    });
  }

  const TRAY_ANIM_DURATION = 300;
  async function animateToTray(): Promise<boolean> {
    if (animatingToTray) return false;
    const result = await withLock(async () => {
      animatingToTray = true;
      state = "hiding";
      try {
        const [pos, wa] = await Promise.all([
          appWindow.outerPosition(),
          getWorkArea(),
        ]);

        if (!wa) {
          await appWindow.hide();
          isHidden.value = true;
          state = "hidden";
          await invoke("set_window_visible", { visible: false });
          return true;
        }

        let shouldSlide = false;
        let startX = pos.x;
        let startY = pos.y;

        if (snappedEdge.value && snapTarget) {
          startX = snapTarget.x;
          startY = snapTarget.y;
          shouldSlide = true;
        }

        savedPosition = { x: startX, y: startY, w: (await appWindow.innerSize()).width as number, h: (await appWindow.innerSize()).height as number };

        if (shouldSlide) {
          const targetX = wa.x + wa.w - 100;
          const targetY = wa.y + wa.h - 20;
          // 使用 Rust 端动画
          await invoke("animate_window_position", {
            targetX, targetY, durationMs: TRAY_ANIM_DURATION,
          });
        }

        await appWindow.hide();
        isHidden.value = true;
        state = "hidden";
        await invoke("save_current_position");
        await invoke("set_window_visible", { visible: false });
        return true;
      } finally {
        animatingToTray = false;
      }
    });
    return result ?? false;
  }

  // 根据窗口位置找到所在的显示器，而不是只用光标所在显示器
  async function getWorkArea() {
    try {
      const [winPos, monitors] = await Promise.all([
        appWindow.outerPosition(),
        availableMonitors(),
      ]);
      if (monitors.length === 0) {
        const monitor = await currentMonitor();
        if (!monitor) return null;
        return {
          x: monitor.workArea.position.x,
          y: monitor.workArea.position.y,
          w: monitor.workArea.size.width,
          h: monitor.workArea.size.height,
          sf: monitor.scaleFactor,
        };
      }
      // 找到包含窗口中心点的显示器
      const winSize = await appWindow.outerSize();
      const centerX = winPos.x + (winSize.width as number) / 2;
      const centerY = winPos.y + (winSize.height as number) / 2;
      for (const m of monitors) {
        const wa = m.workArea;
        if (centerX >= wa.position.x && centerX < wa.position.x + wa.size.width
          && centerY >= wa.position.y && centerY < wa.position.y + wa.size.height) {
          return {
            x: wa.position.x,
            y: wa.position.y,
            w: wa.size.width,
            h: wa.size.height,
            sf: m.scaleFactor,
          };
        }
      }
      // 窗口不在任何显示器内， fallback 到光标所在显示器
      const monitor = await currentMonitor();
      if (!monitor) return null;
      return {
        x: monitor.workArea.position.x,
        y: monitor.workArea.position.y,
        w: monitor.workArea.size.width,
        h: monitor.workArea.size.height,
        sf: monitor.scaleFactor,
      };
    } catch {
      const monitor = await currentMonitor();
      if (!monitor) return null;
      return {
        x: monitor.workArea.position.x,
        y: monitor.workArea.position.y,
        w: monitor.workArea.size.width,
        h: monitor.workArea.size.height,
        sf: monitor.scaleFactor,
      };
    }
  }

  // 检查鼠标是否在窗口外部，margin 乘以 scaleFactor 适配高 DPI
  async function isMouseOutsideWindow(sf = 1): Promise<boolean> {
    // 并行获取鼠标位置和窗口位置，减少时间差
    const [pos, winPos, winSize] = await Promise.all([
      cursorPosition(),
      appWindow.outerPosition(),
      appWindow.outerSize(),
    ]);
    if (!pos) return true;
    const margin = Math.round(20 * sf);
    return pos.x < winPos.x - margin
      || pos.x > winPos.x + (winSize.width as number) + margin
      || pos.y < winPos.y - margin
      || pos.y > winPos.y + (winSize.height as number) + margin;
  }

  // 鼠标是否严格位于窗口边界内（无余量），用于区分"用户点击露出边条"与"系统被动交还焦点"
  async function isMouseInsideWindow(): Promise<boolean> {
    const [pos, winPos, winSize] = await Promise.all([
      cursorPosition(),
      appWindow.outerPosition(),
      appWindow.outerSize(),
    ]);
    if (!pos) return false;
    return pos.x >= winPos.x
      && pos.x <= winPos.x + (winSize.width as number)
      && pos.y >= winPos.y
      && pos.y <= winPos.y + (winSize.height as number);
  }

  async function trySnap(physX: number, physY: number) {
    if (state !== "visible") return;
    const wa = await getWorkArea();
    if (!wa) return;

    // 横纵阈值按各自轴的分辨率比例独立换算
    const thresholdX = scaleX(SNAP_THRESHOLD, wa);
    const thresholdY = scaleY(SNAP_THRESHOLD, wa);
    const pos = await appWindow.outerPosition();
    const size = await appWindow.outerSize();
    const winW = size.width as number;
    const winH = size.height as number;

    const relX = physX - wa.x;
    const relY = physY - wa.y;

    let targetX: number | null = null;
    let targetY: number | null = null;
    let edgeX: "left" | "right" | null = null;
    let edgeY: "top" | "bottom" | null = null;

    // X 和 Y 独立判断，避免角落吸附时互相覆盖
    if (relX <= thresholdX) {
      targetX = wa.x;
      edgeX = "left";
    } else if (Math.abs(relX + winW - wa.w) <= thresholdX) {
      targetX = wa.x + wa.w - winW;
      edgeX = "right";
    }

    if (relY <= thresholdY) {
      targetY = wa.y;
      edgeY = "top";
    } else if (Math.abs(relY + winH - wa.h) <= thresholdY) {
      targetY = wa.y + wa.h - winH;
      edgeY = "bottom";
    }

    // 优先使用 X 边缘（侧边栏场景更常见），其次 Y 边缘
    const edge: SnapEdge = edgeX ?? edgeY;

    if (edge) {
      snappedEdge.value = edge;
      // 进入吸附态即开启内部置顶（不抢焦点）：确保后续所有隐藏/弹出动画全程可见且零闪烁
      appWindow.setAlwaysOnTop(true).catch(() => {});
      const finalX = targetX ?? physX;
      const finalY = targetY ?? physY;
      snapTarget = { x: finalX, y: finalY };
      await animatePosition(finalX, finalY, { x: physX, y: physY });
      // 吸附落位后复核焦点：mouseup 后到 snappedEdge 赋值前有几十毫秒窗口，
      // 此间点击别的软件产生的 blur 会被守卫丢弃（一次性事件不再重发），
      // 这里补救：已失焦且鼠标在窗外立即隐藏，否则交给兜底轮询继续观察
      if (state === "visible" && !(await appWindow.isFocused().catch(() => false))) {
        if (await isMouseOutsideWindow(window.devicePixelRatio || 1)) {
          hideToEdge();
        } else {
          startFocusWatchdog();
        }
      }
    } else {
      snappedEdge.value = null;
      snapTarget = null;
      // 拖离边缘退出吸附态：内部置顶使命结束，恢复用户的置顶设置
      restoreAlwaysOnTop();
    }
  }

  // 弹出后焦点兜底：Windows 前台锁定可能导致 setFocus 静默失败，窗口可见但从未获得焦点，
  // 此后点击任何地方都不会产生 blur 事件，失焦隐藏永远无法触发。
  // 轮询检测「未获焦点且鼠标在窗外」，连续两次命中则主动隐藏；一旦获得焦点即停止，交还给 blur 机制
  function stopFocusWatchdog() {
    if (focusWatchTimer) {
      clearInterval(focusWatchTimer);
      focusWatchTimer = null;
    }
  }

  function startFocusWatchdog() {
    stopFocusWatchdog();
    let missCount = 0;
    focusWatchTimer = setInterval(async () => {
      if (state !== "visible" || !snappedEdge.value) {
        stopFocusWatchdog();
        return;
      }
      try {
        if (await appWindow.isFocused()) {
          stopFocusWatchdog();
          // 弹出后未获焦时保持了临时置顶（见 showFromEdge），现已获焦可安全恢复用户设置
          restoreAlwaysOnTop();
          return;
        }
        // sf 直接取 devicePixelRatio（与窗口所在显示器缩放一致），避免 getWorkArea 的多次 IPC 往返
        if (await isMouseOutsideWindow(window.devicePixelRatio || 1)) {
          missCount++;
          if (missCount >= 2) {
            stopFocusWatchdog();
            hideToEdge();
          }
        } else {
          missCount = 0;
        }
      } catch {
        // 瞬时 IPC 失败忽略，下一轮重试
      }
    }, 120);
  }

  // Z 序策略：窗口实际置顶 = 用户置顶设置 OR 处于吸附状态。
  // 吸附期间内部始终保持置顶：非置顶设置下点击全屏浏览器时，系统会在 blur 送达前
  // 就把浏览器提到最前，事后再补置顶必然有「被盖住→重新浮起」的闪烁，只有全程置顶才能根除。
  // 语义代价几乎为零：吸附模式下失焦即自动隐藏，「非置顶」真正生效的场景只剩自由浮动状态。
  // 真值从后端 get_settings 读取：toggle_always_on_top 实时写后端，前端副本可能过期
  async function restoreAlwaysOnTop() {
    // 吸附期间不降级：保持内部置顶，直到拖离边缘取消吸附才恢复用户设置
    if (snappedEdge.value) return;
    try {
      const s = await invoke<{ always_on_top?: boolean }>("get_settings");
      if (s.always_on_top === false) {
        await appWindow.setAlwaysOnTop(false);
      }
    } catch {
      // 读取设置失败时保持置顶：宁可临时多置顶，也不让窗口沉到其它窗口后面
    }
  }

  async function hideToEdge() {
    if (state !== "visible" || !snappedEdge.value) return;
    stopFocusWatchdog();
    return withLock(async () => {
      try {
      state = "hiding";
      // 终止可能仍在运行的吸附/弹出动画：否则两个 Rust 动画线程会争抢窗口位置，
      // 旧动画结束时的强制落位还会把已隐藏的窗口拉回可见位（吸附后失焦不隐藏的元凶之一）
      await invoke("cancel_window_animation").catch(() => {});
      // 注：不在此处调 setAlwaysOnTop(true)——吸附期间窗口已恒置顶（trySnap 时设置），
      // 重复置顶是冗余的 Z 序重排，会让下层普通窗口整窗重绘闪烁
      const wa = await getWorkArea();
    if (!wa) { state = "visible"; await restoreAlwaysOnTop(); return; }

    const size = await appWindow.outerSize();
    const winW = size.width as number;
    const winH = size.height as number;

    // 如果 snapTarget 不存在，用当前窗口位置作为起点
    let safeX = snapTarget?.x;
    let safeY = snapTarget?.y;
    if (safeX === undefined || safeY === undefined) {
      const curPos = await appWindow.outerPosition();
      safeX = curPos.x;
      safeY = curPos.y;
    }

    const actualPos = await appWindow.outerPosition();
    savedPosition = { x: safeX, y: safeY, w: winW, h: winH };
    const hiddenPx = hiddenPxFor(snappedEdge.value, wa);

      let x = 0;
      let y = 0;

      switch (snappedEdge.value) {
        case "left":
          x = wa.x - (winW - hiddenPx);
          y = safeY;
          break;
        case "right":
          x = wa.x + wa.w - hiddenPx;
          y = safeY;
          break;
        case "top":
          x = safeX;
          y = wa.y - (winH - hiddenPx);
          break;
        case "bottom":
          x = safeX;
          y = wa.y + wa.h - hiddenPx;
          break;
      }

      // 弹出/吸附动画被中途取消时窗口可能停在滑出路径半路上：此时直接从当前位置滑走，
      // 避免先跳回完全展开位再隐藏的视觉跳变；不在路径上才先对齐到起点
      const onSlidePath = snappedEdge.value === "left" || snappedEdge.value === "right"
        ? Math.abs(actualPos.y - safeY) <= 2 && between(actualPos.x, safeX, x)
        : Math.abs(actualPos.x - safeX) <= 2 && between(actualPos.y, safeY, y);
      if (!onSlidePath) {
        await appWindow.setPosition(new PhysicalPosition(safeX, safeY));
      }
      await invoke("animate_window_position", {
        targetX: x, targetY: y, durationMs: ANIM_DURATION,
      });
      // Cache the position for toggle_window
      await invoke("save_hide_position", { x: actualPos.x, y: actualPos.y });
      isHidden.value = true;
      state = "hidden";
      // 启动 Rust 端悬停检测，替代前端轮询
      await startRustHoverDetection(wa, x, y, winW, winH);
      await invoke("set_window_visible", { visible: false });
      // 滑出完成，恢复用户的置顶设置（已完全离屏，Z 序变化无视觉影响）
      await restoreAlwaysOnTop();
      } catch (e) {
        console.error("hideToEdge error:", e);
        state = "visible";
        // 隐藏失败回滚为可见，同步恢复临时置顶，避免非置顶设置下残留置顶态
        await restoreAlwaysOnTop();
      }
    });
  }

  // 调用 Rust 端悬停检测，检测到后通过 "hover-edge-detected" 事件通知前端
  async function startRustHoverDetection(
    wa: { x: number; y: number; w: number; h: number; sf: number },
    hiddenX: number, hiddenY: number, winW: number, winH: number,
  ) {
    if (!snappedEdge.value || !savedPosition) return;
    const hiddenPx = hiddenPxFor(snappedEdge.value, wa);
    // 悬停范围与边方向一致：左右边缘按宽度比例，上下按高度比例
    const hoverRange = snappedEdge.value === "top" || snappedEdge.value === "bottom"
      ? scaleY(HOVER_RANGE, wa)
      : scaleX(HOVER_RANGE, wa);
    await invoke("start_hover_detection", {
      edge: snappedEdge.value,
      winX: savedPosition.x, winY: savedPosition.y,
      winW, winH,
      waX: wa.x, waY: wa.y, waW: wa.w, waH: wa.h,
      hiddenPx, hoverRange,
    });
  }

  async function showFromEdge(focusDelay = 0) {
    if (state !== "hidden" || showingFromEdge) return;
    showingFromEdge = true;
    pendingHideOnShown = null;
    await withLock(async () => {
      try {
        state = "showing";

        const wa = await getWorkArea();
        if (!wa) {
          await appWindow.show();
          state = "visible";
          await invoke("set_window_visible", { visible: true });
          return;
        }

      const targetX = savedPosition?.x ?? 0;
      const targetY = savedPosition?.y ?? 0;

      let startX = targetX;
      let startY = targetY;
      const hiddenPx = hiddenPxFor(snappedEdge.value, wa);

      if (snappedEdge.value) {
        const winW = savedPosition?.w ?? 300;
        const winH = savedPosition?.h ?? 600;
        switch (snappedEdge.value) {
          case "left":   startX = wa.x - (winW - hiddenPx); break;
          case "right":  startX = wa.x + wa.w - hiddenPx; break;
          case "top":    startY = wa.y - (winH - hiddenPx); break;
          case "bottom": startY = wa.y + wa.h - hiddenPx; break;
        }
      }

      // 注：不在此处调 setAlwaysOnTop(true)——吸附期间窗口已恒置顶（trySnap 时设置，隐藏期间也不降级），
      // 重复置顶是冗余 Z 序重排；快速连续呼出/隐藏时它会偶发让下层普通窗口整窗重绘闪烁
      await appWindow.setPosition(new PhysicalPosition(startX, startY));
      await appWindow.show();
      // 提前同步可见标志：弹出动画期间按快捷键，Rust 端即可判定「已可见→应隐藏」，
      // 避免被当成「再次显示」导致需要按两次才能隐藏
      await invoke("set_window_visible", { visible: true });
      await animatePosition(targetX, targetY, undefined, true);

      isHidden.value = false;
      savedPosition = null;
      stopHoverPoll();
      state = "visible";
      } catch (e) {
        console.error("showFromEdge error:", e);
        // Reset to hidden so the state machine doesn't get stuck
        state = "hidden";
      } finally {
        showingFromEdge = false;
      }
    });

    // 入口守卫把 state 收窄为 "hidden"，但锁内闭包已修改它，需断言回完整类型
    if ((state as SnapState) !== "visible") {
      pendingHideOnShown = null;
      restoreAlwaysOnTop();
      return;
    }

    // 弹出期间收到过隐藏请求：explicit（快捷键）直接补执行隐藏；blur（点了别的软件）复核焦点与鼠标位置后隐藏。
    // 两种情况都不再 setFocus，避免从用户正在使用的软件手里抢走前台
    if (pendingHideOnShown) {
      const kind = pendingHideOnShown;
      pendingHideOnShown = null;
      if (kind === "explicit") {
        hideToEdge();
        return;
      }
      const focused = await appWindow.isFocused().catch(() => false);
      if (!focused && (await isMouseOutsideWindow(window.devicePixelRatio || 1))) {
        hideToEdge();
        return;
      }
      // 复核后决定不隐藏：弹出动画可能已被取消停在半路，把窗口对齐到完全展开位
      if (snapTarget) {
        await appWindow.setPosition(new PhysicalPosition(snapTarget.x, snapTarget.y)).catch(() => {});
      }
    }

    if (focusDelay > 0) {
      // 延迟聚焦路径（提醒弹窗）：保持原行为，不启用兜底轮询，避免提醒在无人操作时被自动收起
      setTimeout(() => appWindow.setFocus().catch(() => {}), focusDelay);
      await restoreAlwaysOnTop();
    } else {
      await appWindow.setFocus().catch(() => {});
      // setFocus 可能被系统拒绝（前台锁定）且不报错：确认一次，失败则短延迟后重试一次
      let focused = await appWindow.isFocused().catch(() => false);
      if (!focused) {
        await new Promise((r) => setTimeout(r, 50));
        await appWindow.setFocus().catch(() => {});
        focused = await appWindow.isFocused().catch(() => false);
      }
      // 仍未获得焦点才启动兜底轮询，防止窗口卡在「可见但永远收不到 blur」的状态；
      // 已聚焦则完全交给 blur 机制，零轮询开销
      if (!focused) startFocusWatchdog();
      if (focused) {
        // 已获焦点（焦点窗口本就位于 Z 序顶部），可安全恢复用户的置顶设置；
        // 未获焦点则保持临时置顶：此刻恢复会让窗口立即沉回全屏窗口下面（等于白弹出），
        // 交给兜底轮询——用户不操作时会隐藏，hideToEdge 完成后统一恢复置顶设置
        await restoreAlwaysOnTop();
      }
    }
  }

  // 监听 Rust 端发送的悬停检测事件
  let unlistenHover: (() => void) | null = null;

  // 防止 showFromEdge 被并发调用的标志
  let showingFromEdge = false;

  async function setupHoverListener() {
    unlistenHover = await appWindow.listen("hover-edge-detected", () => {
      // 只有在 hidden 状态且没有正在执行的 showFromEdge 时才触发
      if (state === "hidden" && snappedEdge.value && !showingFromEdge) {
        showFromEdge();
      }
    });
  }

  function startHoverPoll() {
    // 前端不再轮询，由 Rust 端后台线程检测
    // 只需确保事件监听器已注册
  }

  function stopHoverPoll() {
    // Rust 端检测线程会自行终止（窗口显示时退出循环）
  }

  async function onTitleBarMouseDown(e: MouseEvent) {
    if (e.button !== 0 || state !== "visible") return;
    e.preventDefault();

    const pos = await appWindow.outerPosition();
    dragStartMouse = { x: e.screenX, y: e.screenY };
    dragStartWin = { x: pos.x, y: pos.y };
    isDragging = true;
  }

  async function onMouseMove(e: MouseEvent) {
    if (!isDragging || !dragStartMouse || !dragStartWin) return;

    // e.screenX/Y 是 CSS 逻辑像素，窗口位置是物理像素，
    // 高 DPI（如 4K 150%/200% 缩放）下必须乘 devicePixelRatio 换算，否则窗口跟不上鼠标
    const dpr = window.devicePixelRatio || 1;
    const dx = Math.round((e.screenX - dragStartMouse.x) * dpr);
    const dy = Math.round((e.screenY - dragStartMouse.y) * dpr);

    await appWindow.setPosition(new PhysicalPosition(
      dragStartWin.x + dx,
      dragStartWin.y + dy,
    ));
  }

  async function onMouseUp() {
    if (!isDragging) return;
    isDragging = false;
    dragStartMouse = null;

    const pos = await appWindow.outerPosition();
    await trySnap(pos.x, pos.y);
  }

  function onDoubleClickHide() {
    if (state === "visible" && snappedEdge.value) {
      hideToEdge();
    }
  }

  // 图片查看器关闭检测：监听 Rust 端 FindWindowW 事件（无定时器）
  let unlistenImageViewerClosed: (() => void) | null = null;

  onMounted(async () => {
    await setupHoverListener();
    await setupImageViewerListener();

    const titleBar = document.getElementById("title-bar");
    if (titleBar) {
      titleBar.addEventListener("mousedown", onTitleBarMouseDown);
      titleBar.addEventListener("dblclick", onDoubleClickHide);
    }
    const sideNav = document.querySelector(".side-nav");
    if (sideNav) {
      sideNav.addEventListener("dblclick", onDoubleClickHide);
    }
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);

    unlistenFocus = await appWindow.onFocusChanged(async ({ payload }) => {
      if (!payload && state === "visible" && snappedEdge.value) {
        // 失焦时检查鼠标是否在窗口外：
        // - 点击图片打开查看器时，鼠标在窗口内 → 不隐藏 ✓
        // - 关闭查看器后点击别处，鼠标在窗口外 → 隐藏 ✓
        if (await isMouseOutsideWindow(window.devicePixelRatio || 1)) {
          hideToEdge();
        }
      } else if (!payload && state === "showing" && snappedEdge.value) {
        // 弹出动画中失焦（弹出瞬间点了别的软件）：hideToEdge 会被状态守卫丢弃，
        // 标记挂起待弹出完成后复核并补执行隐藏（那边也走定格隐藏）；
        // 同时立即取消弹出动画让 showFromEdge 尽快返回，否则要等弹出动画跑完才能隐藏（感知为延迟）
        if (!pendingHideOnShown) pendingHideOnShown = "blur";
        invoke("cancel_window_animation").catch(() => {});
      } else if (payload && (state === "hidden" || state === "hiding")) {
        // 其它窗口最小化/关闭/切换时，系统会把焦点被动交还给本窗口（隐藏时边条仍置顶可见），
        // 不能无条件弹出；只有鼠标确实在边条上（用户主动点击唤醒）才展开
        if (!showingFromEdge && (await isMouseInsideWindow())) {
          showFromEdge();
        }
      }
    });

    unlistenToggle = await appWindow.listen("toggle-window", async () => {
      if (state === "hidden") {
        showFromEdge();
      } else if (state === "visible") {
        // 状态失步兜底（Rust 认为隐藏、前端认为可见）：确保窗口真实可见并取焦；
        // Rust 端发事件前已将 window_visible 置为 true，两侧重新对齐
        try {
          await appWindow.show();
          await appWindow.setFocus();
          isHidden.value = false;
        } catch (e) {
          console.error("[toggle-window] fallback show error:", e);
        }
      }
      // "showing"/"hiding" 动画中：状态机自身会收敛，此处不干预，避免与动画争抢窗口
    });

    unlistenHide = await appWindow.listen("request-hide", async () => {
      if (state === "visible" || state === "showing") {
        if (snappedEdge.value) {
          if (state === "showing") {
            // 弹出动画中收到隐藏请求：直接调 hideToEdge 会被状态守卫丢弃（且已与 Rust 端 window_visible 失步），
            // 改为挂起并取消弹出动画，弹出流程尽快结束后无条件补执行
            pendingHideOnShown = "explicit";
            invoke("cancel_window_animation").catch(() => {});
          } else {
            hideToEdge();
          }
        } else {
          const [pos, size] = await Promise.all([appWindow.outerPosition(), appWindow.innerSize()]);
          savedPosition = { x: pos.x, y: pos.y, w: size.width as number, h: size.height as number };
          await appWindow.hide();
          isHidden.value = true;
          state = "hidden";
          await invoke("set_window_visible", { visible: false });
        }
      }
    });

  });

  async function setupImageViewerListener() {
    // 监听 Rust 端检测到图片查看器关闭的事件
    unlistenImageViewerClosed = await appWindow.listen("image-viewer-closed", async () => {
      // 图片查看器已关闭 → 再检查鼠标是否在窗口外，才自动隐藏
      if (state === "visible" && snappedEdge.value) {
        if (await isMouseOutsideWindow(window.devicePixelRatio || 1)) {
          hideToEdge();
        }
      }
    });
  }

  onUnmounted(() => {
    const titleBar = document.getElementById("title-bar");
    if (titleBar) {
      titleBar.removeEventListener("mousedown", onTitleBarMouseDown);
      titleBar.removeEventListener("dblclick", onDoubleClickHide);
    }
    const sideNav = document.querySelector(".side-nav");
    if (sideNav) {
      sideNav.removeEventListener("dblclick", onDoubleClickHide);
    }
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
    unlistenFocus?.();
    unlistenToggle?.();
    unlistenHide?.();
    unlistenHover?.();
    unlistenImageViewerClosed?.();
    stopHoverPoll();
    stopFocusWatchdog();
  });

  async function closeToTrayDirect() {
    if (snappedEdge.value) {
      await hideToEdge();
    } else {
      const [pos, size] = await Promise.all([appWindow.outerPosition(), appWindow.innerSize()]);
      savedPosition = { x: pos.x, y: pos.y, w: size.width as number, h: size.height as number };
      await appWindow.hide();
      isHidden.value = true;
      state = "hidden";
      await invoke("save_current_position");
      await invoke("set_window_visible", { visible: false });
    }
  }

  return { snappedEdge, isHidden, hideToEdge, showFromEdge, animateToTray, closeToTray: closeToTrayDirect };
}