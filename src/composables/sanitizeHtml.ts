import DOMPurify from "dompurify";

// 便签内容经 marked 渲染为 HTML 后通过 v-html 注入到特权 WebView 中，
// 若不净化，用户输入的 <script>、onerror 等会执行并可调用 IPC 命令（读写/删除文件）。
// 统一在此净化：剥离脚本与事件处理属性，仅保留 Markdown 渲染所需的安全标签。

// 允许外链在新标签打开：净化后为 a[target] 补 rel，防止 tabnabbing
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A" && node.getAttribute("target")) {
    node.setAttribute("rel", "noopener noreferrer");
  }
});

export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    // marked（GFM）产出的标签：标题、段落、列表、任务列表复选框、表格、代码块、引用、链接、图片等
    USE_PROFILES: { html: true },
    // 禁止 data: 之外的危险协议由 DOMPurify 默认处理；此处不额外放开
  });
}
