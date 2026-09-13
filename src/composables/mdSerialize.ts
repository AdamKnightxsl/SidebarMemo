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
  // 光标落在一个待删的空白节点里时，删了它光标会跳回编辑器开头——这个节点先留着
  const sel = window.getSelection();
  const anchor = sel?.anchorNode;
  const focus = sel?.focusNode;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const doomed: Text[] = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const t = n as Text;
    if (!/^\s+$/.test(t.textContent || "")) continue;
    if (t === anchor || t === focus) continue;
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

function isBr(node: Node | null | undefined): boolean {
  return !!node && node.nodeType === Node.ELEMENT_NODE && (node as Element).tagName === "BR";
}

function hasContent(node: Node | null): boolean {
  for (let n = node; n; n = n.nextSibling) {
    if (n.nodeType === Node.ELEMENT_NODE) return true;
    if ((n.textContent || "").trim()) return true;
  }
  return false;
}

/** 从 from（含）起把节点搬进紧随其后的同类新块 */
function cutBlock(block: HTMLElement, from: Node): HTMLElement {
  const tail = document.createElement(block.tagName.toLowerCase());
  const moving: Node[] = [];
  for (let n: Node | null = from; n; n = n.nextSibling) moving.push(n);
  moving.forEach((n) => tail.appendChild(n));
  block.after(tail);
  return tail;
}

/**
 * 复选框只能是它所在块的第一个节点。落在段落中间（包括换行出来的第二行）时，
 * 序列化只能把整段抬成一条待办，前面的字会被并进待办正文——
 * 就是「在第二行插入待办，退出编辑后第一行变成了待办」。
 * 这里按「前文一块 / 待办一块 / 后文一块」拆开，待办只吃自己那一行。
 */
function splitBlocksAtBareCheckboxes(root: HTMLElement): void {
  const boxes = Array.from(root.querySelectorAll("input[type=checkbox]")).filter((box) => {
    const block = box.parentElement;
    if (!block || block === root || !/^(P|DIV)$/.test(block.tagName)) return false;
    if (block.closest("li")) return false; // 列表项里的框本来就在行首
    return block.firstElementChild !== box;
  });
  // 从后往前拆，先拆的不会让后拆的节点位置失效
  for (const box of boxes.reverse()) {
    const block = box.parentElement as HTMLElement;
    const tail = cutBlock(block, box);
    if (isBr(block.lastChild)) block.lastChild?.remove();
    if (!hasContent(block.firstChild) && !block.querySelector("img,input")) block.remove();
    // 待办只占自己这一行：后面还有正文就再切一刀，那行不该被并进待办项
    const kids = Array.from(tail.childNodes);
    const br = kids.slice(kids.indexOf(box) + 1).find((n) => isBr(n)) || null;
    if (br && hasContent(br.nextSibling)) {
      const after = br.nextSibling as Node;
      br.remove();
      cutBlock(tail, after);
    }
  }
}

/**
 * 标题行里点「有序/无序」，Chromium 产出的是 <h1><ol><li>…</li></ol></h1>。
 * 这种嵌套交给 flattenBlocksInHeading 会被合并回标题（那是「列表项里点标题」要的语义），
 * 用户看到的就是点了列表按钮毫无反应。列表命令这条路径先调本函数：让列表吃掉标题行，
 * 和段落点列表的结果一致；标题里列表之外的文字按原顺序包成段落留在原位。
 */
export function releaseListsFromHeading(root: HTMLElement): void {
  for (const h of Array.from(root.querySelectorAll("h1,h2,h3,h4,h5,h6"))) {
    const isList = (n: Node) => n.nodeType === Node.ELEMENT_NODE && /^(UL|OL)$/.test((n as Element).tagName);
    if (!Array.from(h.children).some(isList)) continue;
    const parent = h.parentNode;
    if (!parent) continue;
    let run: HTMLElement | null = null;
    const runs: HTMLElement[] = [];
    for (const node of Array.from(h.childNodes)) {
      if (isList(node)) {
        run = null;
        parent.insertBefore(node, h);
        continue;
      }
      if (!run) {
        run = document.createElement("p");
        parent.insertBefore(run, h);
        runs.push(run);
      }
      run.appendChild(node);
    }
    parent.removeChild(h);
    runs.forEach((r) => { if (!(r.textContent || "").trim() && !r.querySelector("img,input")) r.remove(); });
  }
}

/** 标题行首的编号 / 圆点前缀。只认 1–3 位数字且后面必须跟空白，免得把「2024. 年度总结」这类手打文字吃掉 */
const HEADING_MARKER = /^(?:([0-9]{1,3})([.、])\s+|([·•])\s+)/;

/**
 * 标题行上的「有序」走的是另一条路：markdown 里一行只能是一种块，`# ` 后面的 `1.` 不会被解析成列表，
 * 所以那种软件的产物形态就是「编号写成标题文字」（`## 1. 阿萨德`）。这里按文档顺序把这些标题行的
 * 编号连续重排，删掉中间一条后面自动补上。只在工具栏改完和保存前调用，不进 normalizeEditorDom：
 * 归一化每敲一个字都跑，会把用户正在打的「1. 」抢改掉、光标跟着乱跳。
 */
export function numberHeadings(root: HTMLElement): void {
  let n = 0;
  for (const h of Array.from(root.querySelectorAll("h1,h2,h3,h4,h5,h6"))) {
    const first = h.firstChild;
    if (!first || first.nodeType !== Node.TEXT_NODE) continue;
    const text = first.textContent || "";
    const m = HEADING_MARKER.exec(text);
    if (!m || !m[1]) continue; // 圆点行不参与编号
    n++;
    if (m[1] === String(n)) continue;
    first.textContent = `${n}. ${text.slice(m[0].length)}`;
  }
}

/**
 * 标题里不能嵌列表/段落：markdown 没有「既是标题又是待办」的行。
 * Chromium 的 formatBlock 作用在列表项上会产出 <h1><ul><li>…</li></ul></h1>，
 * 不拆平就序列化成「# - [ ] 文字」，卡片上直接露出 markdown 符号。
 * 这里把块级子节点的行内内容并进标题（文字和加粗都留下）；复选框和换行只能当场丢掉，
 * 留着也是保存时静默消失，不如让用户立刻看到是被标题覆盖了。
 */
function flattenBlocksInHeading(root: HTMLElement): void {
  for (const h of Array.from(root.querySelectorAll("h1,h2,h3,h4,h5,h6"))) {
    h.querySelectorAll("input[type=checkbox], br").forEach((n) => n.remove());
    // 旧版本把这个组合存成了「# - [ ] 文字」，重开时标题里只剩纯文本符号，
    // 结构上拆不出来，只能识别后剥掉，否则这条正文永远修不好
    const firstText = Array.from(h.childNodes).find((n) => n.nodeType === Node.TEXT_NODE);
    if (firstText) {
      firstText.textContent = (firstText.textContent || "").replace(/^\s*[-*+][ \t]+\[[ xX]\][ \t]+/, "");
    }
    for (const b of Array.from(h.children).filter((c) => BLOCK_CHILD.test(c.tagName))) {
      for (const node of Array.from(b.childNodes)) {
        const wrap = node.nodeType === Node.ELEMENT_NODE && /^(LI|P|DIV)$/.test((node as Element).tagName);
        for (const kid of Array.from(wrap ? node.childNodes : [node])) h.appendChild(kid);
      }
      b.remove();
    }
  }
}

/**
 * 编辑区 DOM 归一化。工具栏每执行一条命令跑一次：
 * 非法嵌套不拆掉，后面按回车时 Chromium 会产出更乱的结构，光标和序列化都没法预期。
 */
export function normalizeEditorDom(root: HTMLElement): void {
  flattenBlocksInHeading(root);
  for (const el of Array.from(root.querySelectorAll("p"))) {
    if (Array.from(el.children).some((c) => BLOCK_CHILD.test(c.tagName))) splitAroundBlocks(el);
  }
  splitBlocksAtBareCheckboxes(root);
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

function blockMarkdown(el: Element, inListItem = false): string {
  const holder = document.createElement("div");
  holder.appendChild(el.cloneNode(true));
  const out: string[] = [];
  blocksOf(holder, "", out, inListItem);
  return out.join("\n").trim();
}

/**
 * 段落里仍有块级子元素时按块拆段序列化（归一化之后理论上走不到，兜底用）。
 * 必须拆：列表和紧跟的普通行之间没有空行的话，markdown 的 lazy continuation
 * 会把那一行并进最后一个列表项，用户写的普通文字就成了列表内容。
 */
function splitBlockParagraph(el: Element, indent: string, out: string[], inListItem = false): void {
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
  blocksOf(holder, indent, out, inListItem);
}

/** 行内节点 → markdown 片段。atLineStart 只影响文本节点的行首转义 */
type SerState = { lineStart: boolean; inListItem?: boolean };

function inline(node: Node, state: SerState): string {
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
    const body = blockMarkdown(el, state.inListItem);
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
    // 已经在新行行首再遇到 <br>，说明用户要的是一个空行。写成 "\n" 会在 markdown 里
    // 造出真空行，而 markdown 的空行切断段落，回填编辑器时空行就退化成几像素的段距。
    // 这里写字面 <br>（GFM 原样透传），空行留在同一段内部，两种视图都是完整一行高。
    if (state.lineStart) return "<br>";
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

function wrap(el: Element, open: string, close: string, state: SerState): string {
  const inner = childrenInline(el, { lineStart: state.lineStart, inListItem: state.inListItem });
  state.lineStart = false;
  if (!inner.trim()) return inner;
  return `${open}${inner}${close}`;
}

function childrenInline(el: Element, state: SerState): string {
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

/**
 * marked 判定任务项要求 `[ ]` 后面至少跟一个非空字符，正文为空的待办写成 `- [ ]`
 * 就退化成正文里的原始 markdown 文本（复选框消失、点不动）。零宽字符既不是空白
 * 也不可见，能保住复选框结构。
 */
const EMPTY_TASK_TEXT = "\u200b";

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
  const state: SerState = { lineStart: true, inListItem: true };
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
  const bodyText = text.replace(/^\s+/, "").replace(/\s+$/, "");
  const body = prefix + (box !== null && !bodyText.trim() ? EMPTY_TASK_TEXT : bodyText);
  // 续行与嵌套只对齐到列表标记之后（「- 」两格、「1. 」三格）。
  // 不能用 prefix 长度：待办前缀是 6 格，缩进 4 格以上会被 markdown 当代码块，
  // 嵌套待办会被解析坏，这条正文也就过不了 isRoundTripSafe。
  const pad = " ".repeat(marker.length);
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

function blocksOf(root: Element, indent: string, out: string[], inListItem = false): void {
  for (const el of Array.from(root.children)) {
    const tag = el.tagName;
    if (tag === "UL" || tag === "OL") {
      emitList(el, indent, out);
      out.push("");
      continue;
    }
    const state: SerState = { lineStart: true, inListItem };
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
      blocksOf(el, "", inner, inListItem);
      out.push(...inner.filter((l, i) => l !== "" || i < inner.length - 1).map((l) => (l ? `${indent}> ${l}` : `${indent}>`)), "");
      continue;
    }
    // 段落里还嵌着列表／标题时不能按一段处理，否则列表后面丢空行
    if ((tag === "P" || tag === "DIV") && el.querySelector(":scope > ul, :scope > ol, :scope > h1, :scope > h2, :scope > h3, :scope > h4, :scope > h5, :scope > h6, :scope > blockquote, :scope > pre")) {
      splitBlockParagraph(el, indent, out, inListItem);
      continue;
    }
    // P / DIV（编辑器换行产生的 div）/ 裸文本容器：都按「一段」处理
    let body = childrenInline(el, state);
    // 工具栏插入的待办框是段落里的裸 input（不在 li 内），不认就会整个丢掉；
    // 已经在 li 里时不能再补：liLines 写过前缀，重复就成了「- [ ] - [ ] 甲」。
    const box = inListItem ? null : checkboxOf(el);
    if (box !== null) {
      const text = body.replace(/^\s+/, "");
      body = `- [${box}] ` + (text.trim() ? text : EMPTY_TASK_TEXT);
    }
    // 整块只有换行（Shift+Enter 离开列表留下的光标占位空行）时不能写成 <br>：
    // 独立成行的 <br> 会被 marked 当 HTML 块，第二次序列化又退化成空行，反复保存会漂移。
    if (!body.replace(/<br\s*\/?>/gi, "").trim() && !el.querySelector("img,input")) {
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
