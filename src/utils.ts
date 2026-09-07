/** 判断键盘事件是否处于 IME 输入法组词状态 */
export function isComposing(e: KeyboardEvent): boolean {
  return e.isComposing || e.keyCode === 229;
}

/** 便签正文压成一行摘要，用于 Toast 里辨认是哪条（markdown 标记会干扰阅读，直接去掉） */
export function contentPreview(content: string, max = 18): string {
  const plain = (content || "").replace(/[#*>`~\[\]()!_]/g, " ").replace(/\s+/g, " ").trim();
  if (!plain) return "（空内容）";
  return plain.length > max ? plain.slice(0, max) + "…" : plain;
}

/**
 * 针对 Tauri setup 竞态的重试包装：
 * 当 invoke 报 "state not managed" 时，每 interval 毫秒重试一次，最多 maxRetries 次。
 * 其他错误直接抛出，不重试。
 */
export async function invokeWithRetry<T>(
  fn: () => Promise<T>,
  { maxRetries = 10, interval = 300 }: { maxRetries?: number; interval?: number } = {}
): Promise<T> {
  let lastErr: unknown;
  for (let i = 0; i <= maxRetries; i++) {
    try {
      return await fn();
    } catch (e) {
      lastErr = e;
      const msg = String(e);
      if (!msg.includes("state not managed")) throw e;
      if (i < maxRetries) {
        await new Promise((r) => setTimeout(r, interval));
      }
    }
  }
  throw lastErr;
}

/** 重复提醒的可选项。取值必须和后端 normalize_repeat 接受的集合一致，多一个少一个都会静默失效 */
export const REPEAT_OPTIONS = [
  { value: "", label: "不重复" },
  { value: "daily", label: "每天" },
  { value: "weekly", label: "每周" },
  { value: "monthly", label: "每月" },
] as const;

export type RepeatValue = (typeof REPEAT_OPTIONS)[number]["value"];

/** 库里 monthly 存的是带锚定日的 monthly:DD，比较选中态时统一归一回 monthly */
export function repeatKey(rule: string): RepeatValue {
  if (rule === "daily" || rule === "weekly" || rule === "monthly") return rule;
  if (rule.startsWith("monthly:")) return "monthly";
  return "";
}

/** 重复规则的中文名，无规则时返回空串（调用方据此决定要不要显示） */
export function repeatLabel(rule: string): string {
  const key = repeatKey(rule);
  return key ? REPEAT_OPTIONS.find((o) => o.value === key)!.label : "";
}

/** 快捷输入区的正文模板。存纯字符串，动态部分用占位符，用户改起来才有一一对应的可见文本 */
export interface MemoTemplate {
  name: string;
  content: string;
}

/**
 * 内置模板，同时是设置页「恢复默认」的来源。
 * settings.templates 为 null（从没改过）时前端就套这一套。
 */
export const DEFAULT_TEMPLATES: MemoTemplate[] = [
  { name: "待办清单", content: "## 待办\n- [ ] " },
  {
    name: "订单记录",
    content:
      "## 订单记录\n**日期：** {date}\n**订单号：**\n**平台：** 抖音\n**商品：**\n**金额：**\n**买家ID：**\n\n### 订单状态\n- [ ] 已发货\n- [ ] 已签收\n- [ ] 已完成\n\n### 备注\n",
  },
  {
    name: "售后记录",
    content:
      "## 售后记录\n**日期：** {date}\n**订单号：**\n**商品：**\n**售后类型：** 退款/退货/换货/补偿\n\n### 问题描述\n\n### 处理方案\n\n### 处理结果\n- [ ] 已处理\n- [ ] 待跟进\n",
  },
  { name: "日记", content: "## {date}\n\n### 今天做了\n\n### 感悟\n" },
  { name: "读书笔记", content: "## 《书名》\n\n### 核心观点\n\n### 金句\n\n### 感想\n" },
  { name: "项目计划", content: "## 项目计划\n\n### 目标\n\n### 步骤\n1. \n2. \n3. \n\n### 截止日期\n" },
];

/** 插入时替换占位符：{date} 今天、{time} 当前时分。split/join 而不是正则，占位符里不会有特殊字符 */
export function fillTemplateVars(content: string): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return content
    .split("{date}").join(d.toLocaleDateString("zh-CN"))
    .split("{time}").join(`${pad(d.getHours())}:${pad(d.getMinutes())}`);
}
