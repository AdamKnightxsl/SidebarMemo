import { ref } from "vue";

/**
 * 卡片右键菜单的共享状态。
 *
 * 菜单渲染在每张卡片内部（要直接调用该卡自己的编辑/删除方法，不适合提到列表层再回传），
 * 但同一时刻只允许有一个打开 —— 所以「哪张卡开着」必须放在模块级：
 * 否则在 B 卡上右键时，A 卡的菜单不会关，屏幕上会叠出两个菜单。
 */
const openId = ref<string | null>(null);
const pos = ref({ x: 0, y: 0 });

export function useContextMenu() {
  function open(id: string, x: number, y: number) {
    pos.value = { x, y };
    openId.value = id;
  }

  function close() {
    openId.value = null;
  }

  return { openId, pos, open, close };
}
