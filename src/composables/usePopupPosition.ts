import { ref, type Ref } from "vue";

export type PopupPlacement = "bottom-right" | "top-left";

/**
 * 弹窗定位 composable：根据触发按钮的视口位置计算 fixed 定位样式
 * @param triggerRef 触发按钮的 template ref
 * @param placement 弹出方向（默认 bottom-right）
 * @param gap 与触发按钮的间距（默认 4px）
 */
export function usePopupPosition(
  triggerRef: Ref<HTMLElement | null>,
  placement: PopupPlacement = "bottom-right",
  gap = 4,
) {
  const popupStyle = ref<Record<string, string>>({});

  function updatePosition() {
    const el = triggerRef.value;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    if (placement === "bottom-right") {
      popupStyle.value = {
        position: "fixed",
        top: rect.bottom + gap + "px",
        right: window.innerWidth - rect.right + "px",
      };
    } else {
      // top-left
      popupStyle.value = {
        position: "fixed",
        top: rect.top - gap + "px",
        left: rect.left + "px",
      };
    }
  }

  return { popupStyle, updatePosition };
}
