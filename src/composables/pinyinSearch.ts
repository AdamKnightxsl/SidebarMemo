import { pinyin } from "pinyin-pro";

/**
 * 拼音模糊匹配工具
 * 支持：中文原文匹配、全拼匹配、拼音首字母匹配
 */

/** 获取每个字符对应的拼音音节数组 */
function getSyllables(text: string): string[] {
  const full = pinyin(text, { toneType: "none", type: "array" });
  return full.map((s) => s.toLowerCase());
}

/** 判断 query 是否包含中文字符 */
function hasChinese(s: string): boolean {
  return /[\u4e00-\u9fff]/.test(s);
}

/**
 * 检查 text 是否匹配 query（原文 / 全拼 / 首字母）
 */
export function matchesQuery(text: string, query: string): boolean {
  const lowerText = text.toLowerCase();
  const q = query.toLowerCase().trim();
  if (!q) return true;

  // 1. 直接子串匹配
  if (lowerText.includes(q)) return true;

  // 2. 拼音匹配（全拼 & 首字母）
  if (hasChinese(text)) {
    const syllables = getSyllables(text);
    const fullPy = syllables.join(" ");
    if (fullPy.includes(q)) return true;
    const initials = syllables.map((s) => s[0] || "").join("");
    if (initials.includes(q)) return true;
  }

  return false;
}

/**
 * 在 HTML 文本中高亮匹配 query 的部分
 * 返回包裹了 <mark> 标签的 HTML 字符串
 */
export function highlightInHtml(html: string, query: string): string {
  const q = query.toLowerCase().trim();
  if (!q) return html;

  // 提取 HTML 中的纯文本，并建立 文本位置 → HTML位置 的映射
  const textChars: string[] = [];
  const htmlPositions: number[] = [];
  let inTag = false;

  for (let i = 0; i < html.length; i++) {
    if (html[i] === "<") inTag = true;
    if (!inTag) {
      textChars.push(html[i]);
      htmlPositions.push(i);
    }
    if (html[i] === ">") inTag = false;
  }

  const plainText = textChars.join("");

  // 1. 尝试直接文本匹配
  const lowerPlain = plainText.toLowerCase();
  const idx = lowerPlain.indexOf(q);
  if (idx !== -1) {
    return highlightRange(html, htmlPositions, idx, idx + q.length);
  }

  // 2. 尝试拼音匹配
  if (hasChinese(plainText)) {
    const syllables = getSyllables(plainText);
    const fullPy = syllables.join(" ");
    const pyIdx = fullPy.indexOf(q);
    if (pyIdx !== -1) {
      const pyEnd = pyIdx + q.length;
      // 将拼音位置映射回文本字符位置
      let charStart = -1;
      let charEnd = -1;
      let pyPos = 0;
      for (let i = 0; i < syllables.length; i++) {
        const sStart = pyPos;
        const sEnd = pyPos + syllables[i].length;
        pyPos = sEnd + 1; // +1 for space separator
        // 音节范围与 query 范围有交集 → 该字符需要高亮
        if (sEnd > pyIdx && sStart < pyEnd) {
          if (charStart === -1) charStart = i;
          charEnd = i + 1;
        }
      }
      if (charStart !== -1) {
        return highlightRange(html, htmlPositions, charStart, charEnd);
      }
    }

    // 3. 尝试首字母匹配
    const initials = syllables.map((s) => s[0] || "").join("");
    const iniIdx = initials.indexOf(q);
    if (iniIdx !== -1) {
      const iniEnd = iniIdx + q.length;
      // 首字母位置直接对应字符位置
      return highlightRange(html, htmlPositions, iniIdx, iniEnd);
    }
  }

  return html;
}

/**
 * 将 textChars[start..end) 对应的 HTML 字符用 <mark> 包裹
 */
function highlightRange(
  html: string,
  htmlPositions: number[],
  start: number,
  end: number
): string {
  if (start >= end || start >= htmlPositions.length) return html;
  end = Math.min(end, htmlPositions.length);

  const htmlStart = htmlPositions[start];
  const htmlEnd = htmlPositions[end - 1] + 1;

  return (
    html.slice(0, htmlStart) +
    "<mark>" +
    html.slice(htmlStart, htmlEnd) +
    "</mark>" +
    html.slice(htmlEnd)
  );
}
