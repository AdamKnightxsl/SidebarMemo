import { reactive } from "vue";

// 折叠态缩略图缓存（模块级单例）：切换视图导致卡片重挂载时直接命中缓存，
// 避免每次都通过 IPC 重新加载大图造成的视觉割裂。
// 注意：仅用于内容框折叠时的小缩略图，展开卡片仍显示原图，不走此缓存。
const THUMB_MAX_EDGE = 128; // 缩略图最长边（像素），折叠态显示尺寸小，128 足够清晰

const thumbs = reactive(new Map<string, string>());
// 正在生成的 key，防止并发重复生成
const pending = new Set<string>();
// 生成队列：串行执行，避免多张大图同时解码造成 CPU/内存尖峰
let queue: Promise<void> = Promise.resolve();

function thumbKey(memoId: string, filename: string) {
  return `${memoId}/${filename}`;
}

// 空闲时段调度：缩略图生成属于低优先级任务，
// 不与点击缩略图、打开图片查看器等交互抢主线程
function idle(cb: () => void) {
  if ("requestIdleCallback" in window) {
    (window as unknown as { requestIdleCallback: (cb: () => void, opts?: { timeout: number }) => void })
      .requestIdleCallback(cb, { timeout: 1000 });
  } else {
    setTimeout(cb, 50);
  }
}

// 用 createImageBitmap 缩小原图：Chromium 在工作线程解码，不阻塞 UI 线程。
// 禁止改回 Image + drawImage 同步解码——大图会卡住主窗口渲染进程，
// WebView2 同源窗口（如 viewer）共享渲染进程时会被一并拖慢。
async function downscale(dataUrl: string): Promise<string> {
  const blob = await (await fetch(dataUrl)).blob();
  const bitmap = await createImageBitmap(blob);
  try {
    const maxSide = Math.max(bitmap.width, bitmap.height);
    if (maxSide <= THUMB_MAX_EDGE) return dataUrl;
    const scale = THUMB_MAX_EDGE / maxSide;
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    const ctx = canvas.getContext("2d");
    if (!ctx) return dataUrl;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    // 128px 小画布的 webp 编码只需毫秒级，可接受同步执行
    return canvas.toDataURL("image/webp", 0.8);
  } finally {
    bitmap.close();
  }
}

export function useThumbnailCache() {
  function getThumb(memoId: string, filename: string): string | undefined {
    return thumbs.get(thumbKey(memoId, filename));
  }

  // 原图加载完成后调用：缓存未命中则在空闲时段排队生成缩略图（即发即忘）
  function ensureThumb(memoId: string, filename: string, dataUrl: string) {
    const key = thumbKey(memoId, filename);
    if (thumbs.has(key) || pending.has(key)) return;
    pending.add(key);
    idle(() => {
      queue = queue.then(async () => {
        // 排队期间被 evict（图片已删除）则放弃生成
        if (!pending.has(key)) return;
        try {
          thumbs.set(key, await downscale(dataUrl));
        } catch {
          thumbs.set(key, dataUrl); // 生成失败用原图兜底
        } finally {
          pending.delete(key);
        }
      });
    });
  }

  // 删除图片时同步清除缓存；不传 filename 则清除该便签全部缩略图
  function evictThumb(memoId: string, filename?: string) {
    if (filename) {
      thumbs.delete(thumbKey(memoId, filename));
      pending.delete(thumbKey(memoId, filename));
    } else {
      for (const key of [...thumbs.keys()]) {
        if (key.startsWith(memoId + "/")) thumbs.delete(key);
      }
      for (const key of [...pending]) {
        if (key.startsWith(memoId + "/")) pending.delete(key);
      }
    }
  }

  return { getThumb, ensureThumb, evictThumb };
}
