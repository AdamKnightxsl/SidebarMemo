// 所见即所得编辑器的「富文本 → markdown」回转层。
//
// 这里不追求逐字节还原原始正文：原始串里多余的行尾空格、列表缩进、空行个数
// 在 DOM 里根本不存在，还原不出来。真正要保证的是两条：
//   1) 序列化结果再渲染一次，用户看到的字和结构完全没变；
//   2) 序列化是幂等的（跑第二次不再变），这样反复编辑不会持续漂移。
// 判定由 isRoundTripSafe 负责，不通过的便签直接退回源码编辑，绝不用所见即所得写回。

import { marked } from "marked";
import { sanitizeHtml } from "./sanitizeHtml";

marked.setOptions({ breaks: true, gfm: true });

/** 与查看态走同一条渲染链（marked + 同一份净化），比较结果才有意义 */
function toDom(md: string): HTMLElement {
  const div = document.createElement("div");
  div.innerHTML = sanitizeHtml(marked.parse(md) as string);
  return div;
}

/** 正文 → 编辑器初始 HTML。marked 出的复选框是 disabled 的，编辑态要能直接点勾 */
export function mdToEditorHtml(md: string): string {
  const div = toDom(md);
  div.querySelectorAll("input[type=checkbox]").forEach((box) => box.removeAttribute("disabled"));
  normalizeEditorDom(div);
  return div.innerHTML;
}

/** execCommand 会把块级元素塞进 <p>，这些标签一旦出现就得从段落里拆出来 */
const BLOCK_CHILD = /^(UL|OL|H[1-6]|BLOCKQUOTE|PRE)$/;

/** 段落里混进块级子元素（<p><ul>…</ul>文字</p>）时，按块边界拆成兄弟节点 */
function splitAroundBlocks(p: Element): void {
  const parent = p.parentNode;
  if (!parent) return;
  const pieces: Node[] = [];
  let run: HTMLElement | null = null;
  for (const child of Array.from(p.childNodes)) {
    if (child.nodeType === Node.ELEMENT_NODE && BLOCK_CHILD.test((child as Element).tagName)) {
      run = null;
      pieces.push(child);
      continue;
    }
    if (!run) {
      run = document.createElement("p");
      pieces.push(run);
    }
    run.appendChild(child);
  }
  for (const piece of pieces) {
    const onlyBlank =
      piece.nodeType === Node.ELEMENT_NODE &&
      (piece as Element).tagName === "P" &&
      !(piece.textContent || "").trim();
    if (onlyBlank) continue;
    parent.insertBefore(piece, p);
  }
  parent.removeChild(p);
}

/** 清掉块与块之间的纯空白文本节点；<li>／code／pre 里的空白是正文，不能碰 */
function cleanStrayBlankText(root: HTMLElement): void {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const doomed: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const t = n as Text;
    if (!/^\s+$/.test(t.textContent || "")) continue;
    const parent = t.parentElement;
    if (!parent) continue;
    if (/^(LI|CODE|PRE)$/.test(parent.tagName)) continue;
    // 夹在两个行内元素中间的空格是真字距（marked 出「**a** *b*」就是这种节点），只有贴着块边界才能删
    if (parent !== root && !isBlockNode(t.previousSibling) && !isBlockNode(t.nextSibling)) continue;
    doomed.push(t);
  }
  doomed.forEach((t) => t.remove());
}

function isBlockNode(node: Node | null): boolean {
  return !!node && node.nodeType === Node.ELEMENT_NODE && BLOCK_TAGS.test((node as Element).tagName);
}

/**
 * 编辑区 DOM 归一化。工具栏每执行一条命令跑一次：
 * 非法嵌套不拆掉，后面按回车时 Chromium 会产出更乱的结构，光标和序列化都没法预期。
 */
export function normalizeEditorDom(root: HTMLElement): void {
  for (const el of Array.from(root.querySelectorAll("p"))) {
    if (Array.from(el.children).some((c) => BLOCK_CHILD.test(c.tagName))) splitAroundBlocks(el);
  }
  cleanStrayBlankText(root);
}

/** 正文先渲染再序列化回正文，用于往返比较 */
export function serializeMarkdown(md: string): string {
  return htmlToMarkdown(toDom(md));
}

function renderedText(md: string): string {
  return (toDom(md).textContent || "").replace(/\s+/g, " ").trim();
}

/** 只看文字看不出图片被吃掉（<img> 没有 textContent），签名里必须带上每张图 */
function renderedSignature(md: string): string {
  const dom = toDom(md);
  const imgs = Array.from(dom.querySelectorAll("img"))
    .map((i) => `${i.getAttribute("src") || ""}|${i.getAttribute("alt") || ""}`)
    .join(" ");
  return `${renderedText(md)}\u0000${imgs}`;
}

/** 行内必须转义的字符：这些在 markdown 里有语义，留在文本里会被重新解释 */
const INLINE_ESCAPES = /[\\*_`\[\]<>]/g;

function escapeInline(text: string, atLineStart: boolean): string {
  let out = text.replace(INLINE_ESCAPES, "\\$&");
  if (atLineStart) {
    // 只有行首的 # - + 数字. > 才会被当成块标记；#标签 不带空格不是标题，不用管
    out = out
      .replace(/^(#{1,6})\s/, "\\$1 ")
      .replace(/^([-+*])\s/, "\\$1 ")
      .replace(/^(\d{1,9})([.)])\s/, "$1\\$2 ")
      .replace(/^>/, "\\>");
  }
  return out;
}

/** 行内遇到块级元素（execCommand 的 formatBlock 会把标题塞进 <p>）时单独按块序列化 */
const BLOCK_TAGS = /^(P|DIV|H[1-6]|UL|OL|LI|BLOCKQUOTE|PRE|HR)$/;

function blockMarkdown(el: Element): string {
  const holder = document.createElement("div");
  holder.appendChild(el.cloneNode(true));
  const out: string[] = [];
  blocksOf(holder, "", out);
  return out.join("\n").trim();
}

/**
 * 段落里仍有块级子元素时按块拆段序列化（归一化之后理论上走不到，兜底用）。
 * 必须拆：列表和紧跟的普通行之间没有空行的话，markdown 的 lazy continuation
 * 会把那一行并进最后一个列表项，用户写的普通文字就成了列表内容。
 */
function splitBlockParagraph(el: Element, indent: string, out: string[]): void {
  const holder = document.createElement("div");
  let run: HTMLElement | null = null;
  for (const child of Array.from(el.childNodes)) {
    if (child.nodeType === Node.ELEMENT_NODE && BLOCK_TAGS.test((child as Element).tagName)) {
      run = null;
      holder.appendChild(child.cloneNode(true));
      continue;
    }
    if (!run) {
      run = document.createElement("p");
      holder.appendChild(run);
    }
    run.appendChild(child.cloneNode(true));
  }
  blocksOf(holder, indent, out);
}

/** 行内节点 → markdown 片段。atLineStart 只影响文本节点的行首转义 */
function inline(node: Node, state: { lineStart: boolean }): string {
  if (node.nodeType === Node.TEXT_NODE) {
    const raw = node.textContent || "";
    if (!raw) return "";
    const s = escapeInline(raw, state.lineStart);
    state.lineStart = s.endsWith("\n");
    return s;
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return "";

  const el = node as Element;
  const tag = el.tagName;

  if (BLOCK_TAGS.test(tag)) {
    // 块边界必须还原成换行，否则标题会和后一段挤在同一行
    const body = blockMarkdown(el);
    state.lineStart = true;
    return body ? "\n" + body + "\n" : "\n";
  }

  if (tag === "IMG") {
    // 图片附件在编辑区外面，编辑区里出现的 <img> 只可能来自正文的 ![]()，不吃掉才不会存个空行丢图
    const src = el.getAttribute("src") || "";
    const alt = el.getAttribute("alt") || "";
    const title = el.getAttribute("title");
    state.lineStart = false;
    return title ? `![${alt}](${src} "${title}")` : `![${alt}](${src})`;
  }
  if (tag === "BR") {
    state.lineStart = true;
    return "\n";
  }
  if (tag === "CODE") {
    const t = el.textContent || "";
    // 反引号包裹的内容不能再转义，否则 `a\b` 会多出字面反斜杠
    state.lineStart = false;
    return "`" + t + "`";
  }
  if (tag === "A") {
    const href = el.getAttribute("href") || "";
    const label = childrenInline(el, state);
    state.lineStart = false;
    return `[${label}](${href})`;
  }
  if (tag === "STRONG" || tag === "B") return wrap(el, "**", "**", state);
  if (tag === "EM" || tag === "I") return wrap(el, "*", "*", state);
  if (tag === "DEL" || tag === "S" || tag === "STRIKE") return wrap(el, "~~", "~~", state);
  // markdown 没有下划线语法，只能原样存 <u>；marked 会透传，净化表里也留着 u
  if (tag === "U" || tag === "INS") return wrap(el, "<u>", "</u>", state);

  // span / mark / font 等纯装饰容器：直接穿透，保住里面的文本
  return childrenInline(el, state);
}

function wrap(el: Element, open: string, close: string, state: { lineStart: boolean }): string {
  const inner = childrenInline(el, { lineStart: state.lineStart });
  state.lineStart = false;
  if (!inner.trim()) return inner;
  return `${open}${inner}${close}`;
}

function childrenInline(el: Element, state: { lineStart: boolean }): string {
  let out = "";
  for (const child of Array.from(el.childNodes)) out += inline(child, state);
  return out;
}

/** 列表项里的复选框：直接读 DOM 的 checked，不再依赖「第几个框」的下标协议 */
function checkboxOf(li: Element): "x" | " " | null {
  const box = li.querySelector(":scope > input[type=checkbox], :scope > p > input[type=checkbox]");
  if (!box) return null;
  return (box as HTMLInputElement).checked ? "x" : " ";
}

/** 一个 <ul>/<ol> → markdown 列表块。父项行写完才会轮到它，嵌套项因此排在父项之后 */
function emitList(list: Element, indent: string, out: string[]): void {
  // 编辑器里拆过行的有序列表带 start，按序号而不是按下标输出，不然「3.」会被改写成「1.」
  const start = list.tagName === "OL" ? Number(list.getAttribute("start")) || 1 : 1;
  Array.from(list.children).forEach((li, i) => {
    if (li.tagName !== "LI") return;
    liLines(li, list.tagName === "UL" ? "- " : `${start + i}. `, indent, out);
  });
}

function liLines(el: Element, marker: string, indent: string, out: string[]): void {
  const box = checkboxOf(el);
  const state = { lineStart: true };
  const nested: Element[] = [];
  let text = "";
  for (const child of Array.from(el.childNodes)) {
    // 嵌套列表单独走块级，混在行内会把子项吞成一行
    if (child.nodeType === Node.ELEMENT_NODE && /^(UL|OL)$/.test((child as Element).tagName)) {
      nested.push(child as Element);
      continue;
    }
    text += inline(child, state);
  }
  // marked 在 <input> 后面留了一个空格，不吃掉就会变成「- [ ]  文字」
  const prefix = box === null ? marker : `- [${box}] `;
  const body = prefix + text.replace(/^\s+/, "").replace(/\s+$/, "");
  // 续行与嵌套都要对齐到标记之后：「- 」两格、「1. 」三格，缩进不对会被重新解析成续行
  const pad = " ".repeat(prefix.length);
  const lines = body.split("\n");
  out.push(indent + lines[0]);
  for (const rest of lines.slice(1)) {
    const body2 = rest.trim();
    out.push(body2 ? indent + pad + body2 : "");
  }
  for (const sub of nested) {
    emitList(sub, indent + pad, out);
  }
}

function blocksOf(root: Element, indent: string, out: string[]): void {
  for (const el of Array.from(root.children)) {
    const tag = el.tagName;
    if (tag === "UL" || tag === "OL") {
      emitList(el, indent, out);
      out.push("");
      continue;
    }
    const state = { lineStart: true };
    if (tag === "HR") {
      out.push(indent + "---", "");
      continue;
    }
    const heading = /^H([1-6])$/.exec(tag);
    if (heading) {
      out.push(indent + "#".repeat(Number(heading[1])) + " " + childrenInline(el, state).trim(), "");
      continue;
    }
    if (tag === "PRE") {
      // 围栏内的行原样输出：再缩进就会每次序列化都右移一格
      const code = (el.textContent || "").replace(/\n$/, "");
      // 语言标记只剩 class 上有，不读回来 ```js 就变成 ``` 了
      const lang = /(?:^|\s)language-([\w-]+)/.exec(el.querySelector("code")?.className || "")?.[1] || "";
      out.push("```" + lang, ...code.split("\n"), "```", "");
      continue;
    }
    if (tag === "BLOCKQUOTE") {
      const inner: string[] = [];
      blocksOf(el, "", inner);
      out.push(...inner.filter((l, i) => l !== "" || i < inner.length - 1).map((l) => (l ? `${indent}> ${l}` : `${indent}>`)), "");
      continue;
    }
    // 段落里还嵌着列表／标题时不能按一段处理，否则列表后面丢空行
    if ((tag === "P" || tag === "DIV") && el.querySelector(":scope > ul, :scope > ol, :scope > h1, :scope > h2, :scope > h3, :scope > h4, :scope > h5, :scope > h6, :scope > blockquote, :scope > pre")) {
      splitBlockParagraph(el, indent, out);
      continue;
    }
    // P / DIV（编辑器换行产生的 div）/ 裸文本容器：都按「一段」处理
    let body = childrenInline(el, state);
    // 工具栏插入的待办框是段落里的裸 input（不在 li 内），不认就会整个丢掉
    const box = checkboxOf(el);
    if (box !== null) body = `- [${box}] ` + body.replace(/^\s+/, "");
    if (body.trim() === "" && !el.querySelector("img,input")) {
      out.push("");
      continue;
    }
    out.push(...body.replace(/^\n+/, "").split("\n").map((l) => (l ? indent + l : "")));
    if (tag === "P" || tag === "DIV") out.push("");
  }
}

/** 编辑器 DOM（或任意渲染结果）→ markdown */
export function htmlToMarkdown(root: Element): string {
  const out: string[] = [];
  const state = { lineStart: true };
  // 根级的裸文本/裸 <br> 先攒成一段，再交给块级处理其余节点
  let lead = "";
  for (const child of Array.from(root.childNodes)) {
    if (child.nodeType === Node.ELEMENT_NODE && /^(P|DIV|UL|OL|H[1-6]|PRE|BLOCKQUOTE|HR)$/.test((child as Element).tagName)) break;
    lead += inline(child, state);
  }
  if (lead.trim()) out.push(lead, "");

  const rest = root.cloneNode(true) as HTMLElement;
  let cut = 0;
  for (const child of Array.from(rest.childNodes)) {
    if (child.nodeType === Node.ELEMENT_NODE && /^(P|DIV|UL|OL|H[1-6]|PRE|BLOCKQUOTE|HR)$/.test((child as Element).tagName)) break;
    child.remove();
    cut++;
  }
  if (cut || lead) blocksOf(rest, "", out);
  else blocksOf(root, "", out);

  return out.join("\n").replace(/\n{3,}/g, "\n\n").replace(/[ \t]+$/gm, "").trim();
}

/**
 * 这条正文能不能安全地用所见即所得编辑并写回。
 * 不看的不是「和原文一字不差」，而是：序列化两次不变，且渲染出来的文字不变。
 */
export function isRoundTripSafe(md: string): boolean {
  const src = md || "";
  if (!src.trim()) return true;
  let once: string;
  try {
    once = serializeMarkdown(src);
  } catch {
    return false;
  }
  try {
    const twice = serializeMarkdown(once);
    if (once !== twice) return false;
    return renderedSignature(src) === renderedSignature(once);
  } catch {
    return false;
  }
}
