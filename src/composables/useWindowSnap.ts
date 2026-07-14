import { ref, onMounted, onUnmounted } from "vue";
import {
  getCurrentWindow,
  currentMonitor,
  cursorPosition,
  PhysicalPosition,
  PhysicalSize,
} from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

export type SnapEdge = "top" | "bottom" | "left" | "right" | null;
type SnapState = "visible" | "hidden" | "showing" | "hiding";

const SNAP_THRESHOLD = 40;
const HIDDEN_PX = 5;
const HOVER_RANGE = 1;
const POLL_INTERVAL = 100;
const ANIM_DURATION = 200;

// 图片查看器使用独立静态页，不参与主窗口吸附逻辑。

export function useWindowSnap() {
  const snappedEdge = ref<SnapEdge>(null);
  const isHidden = ref(false);
  const appWindow = getCurrentWindow();

  let state: SnapState = "visible";
  let savedPosition: { x: number; y: number; w: number; h: number } | null = null;
  let unlistenFocus: (() => void) | null = null;
  let unlistenToggle: (() => void) | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  let dragStartMouse: { x: number; y: number } | null = null;
  let dragStartWin: { x: number; y: number } | null = null;
  let isDragging = false;
  let animatingToTray = false;

  async function animatePosition(targetX: number, targetY: number) {
    const pos = await appWindow.outerPosition();
    const startX = pos.x;
    const startY = pos.y;
    const dx = targetX - startX;
    const dy = targetY - startY;
    const startTime = performance.now();

    await new Promise<void>((resolve) => {
      function step(now: number) {
        const elapsed = now - startTime;
        const t = Math.min(elapsed / ANIM_DURATION, 1);
        const ease = 1 - Math.pow(1 - t, 3);
        appWindow.setPosition(new PhysicalPosition(
          Math.round(startX + dx * ease),
          Math.round(startY + dy * ease),
        ));
        t < 1 ? requestAnimationFrame(step) : resolve();
      }
      requestAnimationFrame(step);
    });
  }

  const TRAY_ANIM_DURATION = 300;

  async function animateToTray(): Promise<boolean> {
    if (animatingToTray) return false;
    animatingToTray = true;

    try {
      await invoke("save_current_position");

      const pos = await appWindow.outerPosition();
      const size = await appWindow.outerSize();
      const origW = size.width as number;
      const origH = size.height as number;
      const startX = pos.x;
      const startY = pos.y;

      const wa = await getWorkArea();
      if (!wa) {
        await invoke("close_to_tray");
        return true;
      }

      const targetX = wa.x + wa.w - 100;
      const targetY = wa.y + wa.h - 20;
      const dx = targetX - startX;
      const dy = targetY - startY;
      const startTime = performance.now();

      await new Promise<void>((resolve) => {
        function step(now: number) {
          const elapsed = now - startTime;
          const t = Math.min(elapsed / TRAY_ANIM_DURATION, 1);
          const ease = 1 - Math.pow(1 - t, 3);
          const inv = 1 - ease;

          appWindow.setPosition(new PhysicalPosition(
            Math.round(startX + dx * ease),
            Math.round(startY + dy * ease),
          ));
          appWindow.setSize(new PhysicalSize(
            Math.max(1, Math.round(origW * inv)),
            Math.max(1, Math.round(origH * inv)),
          ));

          t < 1 ? requestAnimationFrame(step) : resolve();
        }
        requestAnimationFrame(step);
      });

      await appWindow.hide();
      await appWindow.setSize(new PhysicalSize(origW, origH));
      await appWindow.setPosition(new PhysicalPosition(startX, startY));
      invoke("set_window_visible", { visible: false });
      return true;
    } finally {
      animatingToTray = false;
    }
  }

  async function getWorkArea() {
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

  async function isMouseOutsideWindow(): Promise<boolean> {
    const pos = await cursorPosition();
    if (!pos) return true;
    const winPos = await appWindow.outerPosition();
    const winSize = await appWindow.outerSize();
    const margin = 20;
    return pos.x < winPos.x - margin
      || pos.x > winPos.x + (winSize.width as number) + margin
      || pos.y < winPos.y - margin
      || pos.y > winPos.y + (winSize.height as number) + margin;
  }

  async function trySnap(physX: number, physY: number) {
    if (state !== "visible") return;
    const wa = await getWorkArea();
    if (!wa) return;

    const threshold = SNAP_THRESHOLD * wa.sf;
    const pos = await appWindow.outerPosition();
    const size = await appWindow.outerSize();
    const winW = size.width as number;
    const winH = size.height as number;

    const relX = physX - wa.x;
    const relY = physY - wa.y;

    let targetX: number | null = null;
    let targetY: number | null = null;
    let edge: SnapEdge = null;

    if (relX <= threshold) {
      targetX = wa.x;
      edge = "left";
    } else if (Math.abs(relX + winW - wa.w) <= threshold) {
      targetX = wa.x + wa.w - winW;
      edge = "right";
    }

    if (relY <= threshold) {
      targetY = wa.y;
      edge = "top";
    } else if (Math.abs(relY + winH - wa.h) <= threshold) {
      targetY = wa.y + wa.h - winH;
      edge = "bottom";
    }

    if (edge) {
      snappedEdge.value = edge;
      await appWindow.setPosition(new PhysicalPosition(targetX ?? physX, targetY ?? physY));
    } else {
      snappedEdge.value = null;
    }
  }

  async function hideToEdge() {
    if (state !== "visible" || !snappedEdge.value) return;
    state = "hiding";
    const wa = await getWorkArea();
    if (!wa) { state = "visible"; return; }

    const pos = await appWindow.outerPosition();
    const size = await appWindow.outerSize();
    savedPosition = { x: pos.x, y: pos.y, w: size.width, h: size.height };

    const hiddenPx = HIDDEN_PX * wa.sf;
    const winW = size.width as number;
    const winH = size.height as number;

    let x = 0;
    let y = 0;

    switch (snappedEdge.value) {
      case "left":
        x = wa.x - (winW - hiddenPx);
        y = pos.y;
        break;
      case "right":
        x = wa.x + wa.w - hiddenPx;
        y = pos.y;
        break;
      case "top":
        x = pos.x;
        y = wa.y - (winH - hiddenPx);
        break;
      case "bottom":
        x = pos.x;
        y = wa.y + wa.h - hiddenPx;
        break;
    }

    await animatePosition(x, y);
    isHidden.value = true;
    state = "hidden";
    startHoverPoll();
    invoke("set_window_visible", { visible: false });
  }

  async function showFromEdge(focusDelay = 0) {
    if (state !== "hidden") return;
    state = "showing";

    const x = savedPosition?.x ?? 0;
    const y = savedPosition?.y ?? 0;

    await appWindow.show();
    await animatePosition(x, y);

    isHidden.value = false;
    savedPosition = null;
    stopHoverPoll();
    state = "visible";
    invoke("set_window_visible", { visible: true });

    if (focusDelay > 0) {
      setTimeout(() => appWindow.setFocus(), focusDelay);
    } else {
      await appWindow.setFocus();
    }
  }

  async function checkHoverToShow() {
    if (state !== "hidden" || !snappedEdge.value || !savedPosition) return;
    const wa = await getWorkArea();
    if (!wa) return;

    const pos = await cursorPosition();
    if (!pos) return;

    const hiddenPx = HIDDEN_PX * wa.sf;
    const hoverRange = HOVER_RANGE * wa.sf;

    const winX = savedPosition.x;
    const winY = savedPosition.y;
    const winW = savedPosition.w;
    const winH = savedPosition.h;

    let edgeMatch = false;
    let axisMatch = false;

    switch (snappedEdge.value) {
      case "left":
        edgeMatch = pos.x <= wa.x + hiddenPx + hoverRange;
        axisMatch = pos.y >= winY && pos.y <= winY + winH;
        break;
      case "right":
        edgeMatch = pos.x >= wa.x + wa.w - hiddenPx - hoverRange;
        axisMatch = pos.y >= winY && pos.y <= winY + winH;
        break;
      case "top":
        edgeMatch = pos.y <= wa.y + hiddenPx + hoverRange;
        axisMatch = pos.x >= winX && pos.x <= winX + winW;
        break;
      case "bottom":
        edgeMatch = pos.y >= wa.y + wa.h - hiddenPx - hoverRange;
        axisMatch = pos.x >= winX && pos.x <= winX + winW;
        break;
    }

    if (edgeMatch && axisMatch) {
      await showFromEdge();
    }
  }

  function startHoverPoll() {
    stopHoverPoll();
    pollTimer = setInterval(checkHoverToShow, POLL_INTERVAL);
  }

  function stopHoverPoll() {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
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

    const dx = e.screenX - dragStartMouse.x;
    const dy = e.screenY - dragStartMouse.y;

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

  onMounted(async () => {

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
      if (payload || state !== "visible" || !snappedEdge.value) return;
      if (await isMouseOutsideWindow()) {
        hideToEdge();
      }
    });

    unlistenToggle = await appWindow.listen("toggle-window", () => {
      if (state === "hidden") {
        showFromEdge();
      }
    });
  });

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
    stopHoverPoll();
  });

  return { snappedEdge, isHidden, hideToEdge, showFromEdge, animateToTray };
}
