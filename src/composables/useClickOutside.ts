import { onMounted, onBeforeUnmount } from "vue";

export interface ClickOutsideOptions {
  /** 点击这些选择器内部时不触发回调 */
  ignore: string[];
  /** 点击外部时的回调 */
  onClickOutside: () => void;
  /** 监听的事件类型（默认 click） */
  eventType?: "click" | "mousedown";
}

/**
 * 点击外部关闭 composable：自动挂载/卸载 document 监听
 */
export function useClickOutside(options: ClickOutsideOptions) {
  const { ignore, onClickOutside, eventType = "click" } = options;

  function handler(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (ignore.some((sel) => target.closest(sel))) return;
    onClickOutside();
  }

  onMounted(() => document.addEventListener(eventType, handler));
  onBeforeUnmount(() => document.removeEventListener(eventType, handler));
}
