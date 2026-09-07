// flomo 式行内标签：#标签 直接写在正文里，content 是唯一来源。
// 刻意不落一个 tags 字段——那样每次编辑正文都要同步两份数据，
// 漏一条路径就会出现「列表里筛选得到、卡片上看不到」的对不上情况。
// 同理，「提取标签」和「渲染 chip」必须共用一条规则（findTagRanges + 代码剔除），
// 否则统计列表会冒出卡片上根本没有的标签。

const TAG_CHARS = "\u4e00-\u9fffa-zA-Z0-9_";
// 标签名：中文/字母/数字/下划线开头，可含 - 与 /（多级标签 #工作/售后），最长 30
const TAG_RE = new RegExp(`#([${TAG_CHARS}][${TAG_CHARS}\\-/]{0,29})`, "g");
/**
 * # 前面是这些字符时不算标签：C#、tag#x、URL 锚点 x#a、版本 1.0#2。
 * 中文没有词间空格，「今天发货#订单」里的 # 就是标签的意思，所以汉字与中文标点
 * 不算这里的「标识符字符」，只有拉丁字母、数字、下划线和几个拼接符号才算。
 */
const WORD_BEFORE = /[A-Za-z0-9_\-.+&%/:]/;
// 代码块 / 行内代码里的 #xxx 是注释或指令，不是标签
const CODE_RE = /```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]*`/g;
// 链接目标 [](#锚点) / [文本](#锚点) 里的 # 渲染后进了 href，卡片上不会有 chip
const LINK_TARGET_RE = /\]\([^)]*\)/g;

/**
 * 提取标签前先把「渲染后不会再以纯文本出现 #」的部分归一化掉。
 * 代码段替换成一个占位字母而不是删掉：这样「# 紧跟代码结尾」时，
 * 前后字符的判断结果与在渲染后 HTML 上做同样判断一致。
 */
function taggableText(content: string): string {
  return content.replace(CODE_RE, "x").replace(LINK_TARGET_RE, "]()");
}

export interface TagRange {
  /** # 在待查文本中的下标 */
  start: number;
  name: string;
}

/** 找出 text 中所有 #标签 的位置与名称（边界规则的唯一来源） */
function findTagRanges(text: string): TagRange[] {
  const out: TagRange[] = [];
  TAG_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TAG_RE.exec(text))) {
    const name = trimTag(m[1]);
    if (!name) continue;
    const start = m.index;
    if (start > 0 && WORD_BEFORE.test(text[start - 1])) continue;
    out.push({ start, name });
  }
  return out;
}

/** 正文里出现的所有标签（按出现顺序，去重） */
export function extractTags(content: string): string[] {
  if (!content) return [];
  const out: string[] = [];
  for (const r of findTagRanges(taggableText(content))) {
    if (!out.includes(r.name)) out.push(r.name);
  }
  return out;
}

export function hasTag(content: string, tag: string): boolean {
  return extractTags(content).includes(tag);
}

/** 从 .md-tag 的文本还原标签名（渲染时保留了开头的 #） */
export function tagFromChip(text: string | null): string {
  return trimTag((text || "").replace(/^#/, ""));
}

// 标签名整串校验：与 TAG_RE 共用同一张字符表，两者判定结果必须一致
const TAG_FULL_RE = new RegExp(`^[${TAG_CHARS}][${TAG_CHARS}\\-/]{0,29}$`);

/**
 * 清洗输入框里的标签名：去掉用户顺手打的 # 和首尾空格，再按提取规则校验。
 * 不合法就返回空串——「能写进正文、卡片上却不成 chip」的标签宁可直接拒掉。
 */
export function normalizeTag(raw: string): string {
  const name = trimTag((raw || "").trim().replace(/^#+/, ""));
  return TAG_FULL_RE.test(name) ? name : "";
}

function trimTag(name: string): string {
  return name.replace(/[-/]+$/, "");
}

/**
 * 扫描 HTML，返回纯文本每个字符在原串中的下标，以及它是否落在 <pre>/<code> 内部。
 * marked 会把正文里的 < 转义成 &lt;，所以剩下的 < 基本都是真标签。
 */
function scanTextNodes(html: string): { idx: number[]; inCode: boolean[] } {
  const idx: number[] = [];
  const inCode: boolean[] = [];
  let codeDepth = 0;
  let i = 0;
  while (i < html.length) {
    if (html[i] === "<") {
      const gt = html.indexOf(">", i);
      if (gt !== -1) {
        const tag = html.slice(i + 1, gt);
        const name = /^[ \t]*\/?([a-zA-Z0-9]+)/.exec(tag)?.[1]?.toLowerCase() || "";
        if (name === "pre" || name === "code") {
          if (tag.trimStart().startsWith("/")) codeDepth = Math.max(0, codeDepth - 1);
          else codeDepth++;
        }
        i = gt + 1;
        continue;
      }
    }
    idx.push(i);
    inCode.push(codeDepth > 0);
    i++;
  }
  return { idx, inCode };
}

/** 把渲染后 HTML 里的 #标签 包成 <span class="md-tag">，供卡片点击筛选 */
export function markTagsInHtml(html: string): string {
  if (!html || !html.includes("#")) return html;
  const { idx, inCode } = scanTextNodes(html);
  const plain = idx.map((p) => html[p]).join("");
  const parts: string[] = [];
  let last = 0;
  for (const { start, name } of findTagRanges(plain)) {
    const end = start + 1 + name.length; // 含开头的 #
    if (inCode.slice(start, end).some(Boolean)) continue;
    const htmlStart = idx[start];
    const htmlEnd = idx[end - 1] + 1;
    if (htmlStart < last) continue;
    parts.push(html.slice(last, htmlStart), '<span class="md-tag">', html.slice(htmlStart, htmlEnd), "</span>");
    last = htmlEnd;
  }
  if (last === 0) return html;
  parts.push(html.slice(last));
  return parts.join("");
}
