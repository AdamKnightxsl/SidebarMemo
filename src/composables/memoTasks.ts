// 行内子任务：`- [ ] 事项` 直接写在正文里，content 仍是唯一来源。
// 和 #标签 一样的理由：不再存一份「第几项已完成」字段，否则每次编辑正文都要同步两份数据，
// 漏一条路径就会出现「勾选状态和正文对不上」。
// 代价是「HTML 里的复选框」必须和「正文里的 [ ]」严格一一对应：
// 两边各扫一遍，数量不一致就干脆不生成可点框（保留 marked 原本的只读复选框），
// 宁可这条卡片点不动，也不能点错行把用户正文改坏。

// 代码块 / 行内代码里的 - [ ] 是示例文本，渲染后不会有复选框
const CODE_RE = /```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]*`/g;

/**
 * 一行任务标记，规则对齐 marked 判定列表项是否为任务的正则：
 * 方括号里只能是一个空格或 x/X，后面至少一个空格、再跟非空内容（`- [ ]` 单独一行不算任务）。
 * 缩进只允许 3 空格：4 空格或 Tab 在 Markdown 里是代码块。引用前缀 `>` 可以有，
 * 因为 `> - [ ] x` 渲染后确实带复选框。
 */
const TASK_RE =
  /^ {0,3}(?:>[ \t]{0,3})*(?:[-*+]|\d{1,9}[.)])[ \t]+\[( |[xX])\][ \t]+\S/gm;

const TAG_RE = /<\/?([a-zA-Z][a-zA-Z0-9]*)\b[^>]*>/g;

const TASK_BOX = '<span class="md-task"></span>';
const TASK_DONE = '<span class="md-task done"></span>';

export interface TaskMark {
  /** 方括号内那个标记字符（空格 / x / X）在正文中的下标 */
  boxAt: number;
  checked: boolean;
}

/** 正文里的所有可交互任务项，按出现顺序（渲染与回写的唯一规则来源） */
export function taskMarks(content: string): TaskMark[] {
  if (!content || !/\[[ xX]\]/.test(content)) return [];
  const code: Array<[number, number]> = [];
  CODE_RE.lastIndex = 0;
  let c: RegExpExecArray | null;
  while ((c = CODE_RE.exec(content))) code.push([c.index, c.index + c[0].length]);
  const inCode = (at: number) => code.some(([s, e]) => at >= s && at < e);

  const out: TaskMark[] = [];
  TASK_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TASK_RE.exec(content))) {
    // 匹配串里第一个 [ 就是复选框的左括号（前面的部分只有缩进、引用符和项目符号）
    const at = m.index + m[0].indexOf("[") + 1;
    if (inCode(at)) continue;
    out.push({ boxAt: at, checked: content[at] !== " " });
  }
  return out;
}

/**
 * 把渲染结果里的只读复选框换成 <span class="md-task">，供卡片点击回写正文。
 * 空 span 不引入任何纯文本，所以放在高亮之前也不会让 <mark> 的下标错位。
 * 序号不写 data-* 属性（净化白名单没放开），点击时按 DOM 顺序数第几个即可。
 */
export function markTasksInHtml(html: string, content: string): string {
  if (!html || !html.includes("<input")) return html;
  const marks = taskMarks(content);
  if (marks.length === 0) return html;
  const boxes = checkboxTags(html);
  if (boxes.length !== marks.length) return html;

  const parts: string[] = [];
  let last = 0;
  boxes.forEach((box, i) => {
    parts.push(html.slice(last, box.start), marks[i].checked ? TASK_DONE : TASK_BOX);
    last = box.end;
  });
  parts.push(html.slice(last));
  return parts.join("");
}

/** HTML 里非代码区的 checkbox（marked 的输出固定是 <input disabled="" type="checkbox">） */
function checkboxTags(html: string): Array<{ start: number; end: number }> {
  const out: Array<{ start: number; end: number }> = [];
  let codeDepth = 0;
  TAG_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TAG_RE.exec(html))) {
    const raw = m[0];
    const name = m[1].toLowerCase();
    if (name === "pre" || name === "code") {
      if (raw[1] === "/") codeDepth = Math.max(0, codeDepth - 1);
      else codeDepth++;
      continue;
    }
    if (codeDepth === 0 && name === "input" && /\btype="checkbox"/i.test(raw)) {
      out.push({ start: m.index, end: m.index + raw.length });
    }
  }
  return out;
}

/**
 * 勾选 / 取消第 index 项，返回新正文。
 * index 越界（渲染与正文不同步）时返回 null，调用方据此什么都不改。
 */
export function toggleTask(content: string, index: number): string | null {
  const mark = taskMarks(content)[index];
  if (!mark) return null;
  return content.slice(0, mark.boxAt) + (mark.checked ? " " : "x") + content.slice(mark.boxAt + 1);
}
