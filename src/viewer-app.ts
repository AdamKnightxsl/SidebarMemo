import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ViewerPayload {
  images: string[];
  index: number;
}

function clamp(n: number, min: number, max: number) {
  return Math.min(max, Math.max(min, n));
}

export async function mountViewer(root: HTMLElement) {
  root.innerHTML = `
    <div id="viewer-root" style="
      width:100%;height:100%;display:flex;flex-direction:column;
      background:#111;color:#eee;font-family:system-ui,Segoe UI,sans-serif;user-select:none;overflow:hidden;
    ">
      <div style="
        flex:0 0 44px;display:flex;align-items:center;justify-content:space-between;
        padding:0 12px;background:#1a1a1a;border-bottom:1px solid #2a2a2a;
      ">
        <div style="display:flex;gap:8px;">
          <button id="v-prev" class="v-btn" title="上一张">‹</button>
          <button id="v-next" class="v-btn" title="下一张">›</button>
        </div>
        <div id="v-counter" style="flex:1;text-align:center;color:#aaa;font-size:13px;">0 / 0</div>
        <div style="display:flex;gap:8px;">
          <button id="v-zoom-out" class="v-btn" title="缩小">−</button>
          <button id="v-zoom-reset" class="v-btn" title="重置">100%</button>
          <button id="v-zoom-in" class="v-btn" title="放大">+</button>
          <button id="v-fit" class="v-btn" title="适应">适应</button>
          <button id="v-close" class="v-btn" title="关闭" style="min-width:36px;">✕</button>
        </div>
      </div>
      <div id="v-stage" style="
        flex:1;min-height:0;position:relative;overflow:hidden;cursor:grab;
        display:flex;align-items:center;justify-content:center;background:#111;
      ">
        <img id="v-img" alt="preview" style="display:none;max-width:none;max-height:none;transform-origin:center center;pointer-events:none;" />
        <div id="v-empty" style="color:#aaa;font-size:14px;padding:16px;text-align:center;">加载中…</div>
        <div style="position:absolute;left:50%;bottom:16px;transform:translateX(-50%);font-size:12px;color:#777;pointer-events:none;">
          滚轮缩放 · 拖拽移动 · ← → 切换 · Esc 关闭
        </div>
      </div>
    </div>
    <style>
      .v-btn{
        height:30px;min-width:30px;padding:0 10px;border:none;border-radius:6px;
        background:#2a2a2a;color:#eee;cursor:pointer;font-size:13px;
        display:inline-flex;align-items:center;justify-content:center;
      }
      .v-btn:hover{background:#3a3a3a;}
      .v-btn:disabled{opacity:.35;cursor:default;}
      #v-close:hover{background:#c42b1c;}
      #v-stage.dragging{cursor:grabbing;}
      html,body,#app{width:100%;height:100%;margin:0;overflow:hidden;background:#111;}
    </style>
  `;

  const imgEl = root.querySelector("#v-img") as HTMLImageElement;
  const stageEl = root.querySelector("#v-stage") as HTMLDivElement;
  const emptyEl = root.querySelector("#v-empty") as HTMLDivElement;
  const counterEl = root.querySelector("#v-counter") as HTMLDivElement;
  const btnPrev = root.querySelector("#v-prev") as HTMLButtonElement;
  const btnNext = root.querySelector("#v-next") as HTMLButtonElement;
  const btnZoomIn = root.querySelector("#v-zoom-in") as HTMLButtonElement;
  const btnZoomOut = root.querySelector("#v-zoom-out") as HTMLButtonElement;
  const btnZoomReset = root.querySelector("#v-zoom-reset") as HTMLButtonElement;
  const btnFit = root.querySelector("#v-fit") as HTMLButtonElement;
  const btnClose = root.querySelector("#v-close") as HTMLButtonElement;

  let images: string[] = [];
  let index = 0;
  let scale = 1;
  let offsetX = 0;
  let offsetY = 0;
  let naturalW = 0;
  let naturalH = 0;
  let dragging = false;
  let dragStartX = 0;
  let dragStartY = 0;
  let dragOriginX = 0;
  let dragOriginY = 0;
  let fitMode = true;

  function updateCounter() {
    counterEl.textContent = images.length ? `${index + 1} / ${images.length}` : "0 / 0";
    btnPrev.disabled = images.length <= 1;
    btnNext.disabled = images.length <= 1;
  }

  function applyTransform() {
    imgEl.style.transform = `translate(${offsetX}px, ${offsetY}px) scale(${scale})`;
    btnZoomReset.textContent = `${Math.round(scale * 100)}%`;
  }

  function fitToWindow() {
    if (!naturalW || !naturalH) return;
    const rect = stageEl.getBoundingClientRect();
    const pad = 24;
    scale = clamp(Math.min((rect.width - pad) / naturalW, (rect.height - pad) / naturalH, 1), 0.05, 8);
    offsetX = 0;
    offsetY = 0;
    fitMode = true;
    applyTransform();
  }

  function showImage(i: number) {
    if (!images.length) {
      imgEl.style.display = "none";
      emptyEl.style.display = "block";
      emptyEl.textContent = "没有可预览的图片";
      updateCounter();
      return;
    }
    index = ((i % images.length) + images.length) % images.length;
    updateCounter();
    emptyEl.style.display = "none";
    imgEl.style.display = "block";
    const src = images[index];
    imgEl.onload = () => {
      naturalW = imgEl.naturalWidth || 1;
      naturalH = imgEl.naturalHeight || 1;
      fitToWindow();
    };
    imgEl.onerror = () => {
      emptyEl.style.display = "block";
      emptyEl.textContent = "图片加载失败";
      imgEl.style.display = "none";
    };
    imgEl.src = src;
  }

  function setPayload(payload: ViewerPayload | null | undefined) {
    images = Array.isArray(payload?.images) ? payload!.images.filter(Boolean) : [];
    index = clamp(payload?.index || 0, 0, Math.max(0, images.length - 1));
    showImage(index);
  }

  async function closeViewer() {
    try {
      await invoke("close_image_viewer");
      return;
    } catch { /* fallthrough */ }
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().destroy();
      return;
    } catch { /* fallthrough */ }
    try {
      window.close();
    } catch { /* ignore */ }
  }

  btnClose.onclick = () => { void closeViewer(); };
  btnPrev.onclick = () => showImage(index - 1);
  btnNext.onclick = () => showImage(index + 1);
  btnZoomIn.onclick = () => {
    fitMode = false;
    scale = clamp(scale * 1.2, 0.05, 8);
    applyTransform();
  };
  btnZoomOut.onclick = () => {
    fitMode = false;
    scale = clamp(scale / 1.2, 0.05, 8);
    applyTransform();
  };
  btnZoomReset.onclick = () => {
    fitMode = false;
    scale = 1;
    offsetX = 0;
    offsetY = 0;
    applyTransform();
  };
  btnFit.onclick = () => fitToWindow();

  stageEl.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      if (!images.length) return;
      fitMode = false;
      scale = clamp(scale * (e.deltaY > 0 ? 1 / 1.12 : 1.12), 0.05, 8);
      applyTransform();
    },
    { passive: false },
  );

  stageEl.addEventListener("mousedown", (e) => {
    if (e.button !== 0 || !images.length) return;
    dragging = true;
    stageEl.classList.add("dragging");
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    dragOriginX = offsetX;
    dragOriginY = offsetY;
  });
  window.addEventListener("mousemove", (e) => {
    if (!dragging) return;
    offsetX = dragOriginX + (e.clientX - dragStartX);
    offsetY = dragOriginY + (e.clientY - dragStartY);
    fitMode = false;
    applyTransform();
  });
  window.addEventListener("mouseup", () => {
    dragging = false;
    stageEl.classList.remove("dragging");
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") void closeViewer();
    else if (e.key === "ArrowLeft") showImage(index - 1);
    else if (e.key === "ArrowRight") showImage(index + 1);
    else if (e.key === "+" || e.key === "=") {
      fitMode = false;
      scale = clamp(scale * 1.2, 0.05, 8);
      applyTransform();
    } else if (e.key === "-") {
      fitMode = false;
      scale = clamp(scale / 1.2, 0.05, 8);
      applyTransform();
    } else if (e.key === "0") fitToWindow();
  });
  window.addEventListener("resize", () => {
    if (fitMode) fitToWindow();
  });

  try {
    const payload = await invoke<ViewerPayload | null>("get_viewer_payload");
    setPayload(payload);
  } catch (e) {
    emptyEl.style.display = "block";
    emptyEl.textContent = "加载失败: " + String(e);
  }

  await listen<ViewerPayload>("viewer-payload", (event) => {
    setPayload(event.payload);
  });
}
