/**
 * 集合归属色板。键名必须与 src-tauri/src/db.rs 的 BOARD_COLORS 逐一对应，
 * 色值只在 style.css 的 --board-* 令牌里定义（深浅主题各一档），这里不带任何 hex。
 */
export const BOARD_COLORS = [
  { key: "coral", label: "珊瑚" },
  { key: "amber", label: "琥珀" },
  { key: "lemon", label: "金黄" },
  { key: "grass", label: "草绿" },
  { key: "teal", label: "青碧" },
  { key: "sky", label: "天蓝" },
  { key: "indigo", label: "靛蓝" },
  { key: "violet", label: "紫罗兰" },
  { key: "magenta", label: "洋红" },
  { key: "clay", label: "陶土" },
];
