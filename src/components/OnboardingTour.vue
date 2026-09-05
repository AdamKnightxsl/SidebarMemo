<script setup lang="ts">
import { inject, onBeforeUnmount, onMounted, type Ref } from "vue";
import { useTour } from "../composables/useTour";

const {
  phase, lit, live, sealed, text, hintText,
  spotStyle, sealStyle, wallsStyle, tipStyle, arrowStyle, tipSide,
  rootEl, tipEl, progress, isLastStep, showPrev,
  beginSteps, next, prev, finish, end, remeasure, handleDocumentClick,
} = useTour();

const currentView = inject<Ref<string> | undefined>("currentView", undefined);

function onClickCapture(e: MouseEvent) {
  handleDocumentClick(e);
}

function onKeydown(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null;
  const typing = !!t && (t.tagName === "TEXTAREA" || t.tagName === "INPUT");
  if (e.key === "Escape") return end();
  if (typing) return;
  if (e.key === "ArrowRight") next();
  else if (e.key === "ArrowLeft") prev();
}

function backToMemos() {
  if (currentView) currentView.value = "memos";
  end();
}

// 布局随时在变（列表滚动、输入框自动长高、窗口缩放），框歪了必须立刻重算
function onScroll() {
  remeasure();
}

onMounted(() => {
  document.addEventListener("click", onClickCapture, true);
  document.addEventListener("keydown", onKeydown, true);
  document.addEventListener("scroll", onScroll, true);
  window.addEventListener("resize", remeasure);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onClickCapture, true);
  document.removeEventListener("keydown", onKeydown, true);
  document.removeEventListener("scroll", onScroll, true);
  window.removeEventListener("resize", remeasure);
});
</script>

<template>
  <div class="tour-root" ref="rootEl">
    <template v-if="phase === 'steps'">
      <!-- 遮罩拆成 4 块「墙」绕开聚光区：墙上的点击=进入下一步，
           空洞里的点击原生落到真实控件上（焦点 / hover / 输入法都是真的） -->
      <div v-for="(s, i) in wallsStyle" :key="i" class="wall" :style="s" @click="next"></div>
      <div class="spotlight" :class="{ on: lit, live }" :style="spotStyle"></div>
      <!-- 被动步骤没有可演示的操作，把空洞封住防误点 -->
      <div v-if="sealed" class="hole-seal" :style="sealStyle" @click="next"></div>

      <div class="tip" ref="tipEl" :style="tipStyle" :data-side="tipSide">
        <div class="arrow" :style="arrowStyle"></div>
        <h4>{{ text.title }}</h4>
        <!-- 一条一行，序号走 counter：长条目换行时缩进与序号列对齐 -->
        <ol class="tip-list">
          <li v-for="(line, i) in text.body" :key="i">{{ line }}</li>
        </ol>
        <span v-if="hintText" class="hint">{{ hintText }}</span>
        <div class="tip-bar">
          <span class="tip-progress">{{ progress }}</span>
          <div class="tip-btns">
            <button class="skip-all" @click="finish">跳过引导</button>
            <button v-if="showPrev" class="tip-btn ghost" @click="prev">上一步</button>
            <button class="tip-btn primary" @click="next">{{ isLastStep ? "完成" : "下一步" }}</button>
          </div>
        </div>
      </div>
    </template>

    <!-- 欢迎卡 / 收尾卡：贴边吸附、全局快捷键这些没有 DOM 可指，只能用文字讲 -->
    <Transition name="tour-card">
      <div v-if="phase === 'welcome'" class="center-card">
        <div class="card">
          <h3>欢迎使用 Sidebar Memo</h3>
          <p class="sub">常驻屏幕侧边、随时呼出的轻量备忘录。</p>
          <div class="row"><div class="ico">记</div><div class="tx"><b>底部输入框</b><span>写完回车就存进列表</span></div></div>
          <div class="row"><div class="ico">键</div><div class="tx"><b>Alt + Q</b><span>任意界面呼出窗口</span></div></div>
          <div class="row"><div class="ico">边</div><div class="tx"><b>贴边隐藏</b><span>拖到屏幕边缘自动吸附</span></div></div>
          <details>
            <summary>查看全部功能</summary>
            <ul>
              <li>Markdown：加粗 / 斜体 / 代码 / 列表</li>
              <li>编辑时可粘贴图片，点击放大查看</li>
              <li>拖拽左侧 ⋮ 调整顺序</li>
              <li>删除的备忘在垃圾桶保留 30 天</li>
            </ul>
          </details>
          <div class="card-btns">
            <button class="tip-btn ghost" @click="finish">跳过</button>
            <button class="tip-btn primary" @click="beginSteps">开始引导</button>
          </div>
        </div>
      </div>
    </Transition>

    <Transition name="tour-card">
      <div v-if="phase === 'final'" class="center-card">
        <div class="card">
          <h3>就这些，去写第一条吧</h3>
          <p class="sub">随时可以在「设置 → 引导手册」重播。</p>
          <div class="row"><div class="ico">键</div><div class="tx"><b>Alt + Q</b><span>全局快捷键呼出 / 隐藏窗口</span></div></div>
          <div class="row"><div class="ico">盘</div><div class="tx"><b>托盘图标</b><span>右键可显示窗口或退出程序</span></div></div>
          <div class="row"><div class="ico">色</div><div class="tx"><b>皮肤与暗色</b><span>设置里有 6 套配色</span></div></div>
          <div class="card-btns">
            <button class="tip-btn" @click="backToMemos">再看看列表</button>
            <button class="tip-btn primary" @click="end">开始使用</button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* 引导层全部取主题令牌，不硬编码颜色（主按钮固定烟岚青，与首次引导一致） */
.tour-root {
  position: absolute;
  inset: 0;
  z-index: 100000;
  pointer-events: none;
  /* 再混一次 transparent 降不透明度：两层 color-mix 都是不透明色的话遮罩就是实心的 */
  --tour-scrim: color-mix(in srgb, color-mix(in srgb, var(--bg-primary) 42%, #000) 60%, transparent);
  /* 小字用的强调色：亮皮肤（fresh / ocean）的 accent 直接压在气泡底上对比不足 3:1 */
  --tour-strong: color-mix(in srgb, var(--accent) 62%, var(--text-primary));
}
.tour-root > * { pointer-events: auto; }

/* 遮罩 + 聚光框：同一个元素的超大 box-shadow 既当暗幕又当洞 */
.spotlight {
  position: absolute;
  border-radius: 10px;
  box-shadow: 0 0 0 100vmax var(--tour-scrim);
  outline: 2px solid var(--accent);
  outline-offset: 0;
  opacity: 0;
  transition: opacity .3s ease,
              top .34s cubic-bezier(.4, 0, .2, 1),
              left .34s cubic-bezier(.4, 0, .2, 1),
              width .34s cubic-bezier(.4, 0, .2, 1),
              height .34s cubic-bezier(.4, 0, .2, 1);
  pointer-events: none !important;
}
.spotlight.on { opacity: 1; }
.spotlight.live { animation: tour-glow 1.6s ease-in-out infinite; }
@keyframes tour-glow {
  0%, 100% { outline-color: var(--accent); }
  50% { outline-color: color-mix(in srgb, var(--accent) 45%, transparent); }
}
@media (prefers-reduced-motion: reduce) {
  .spotlight.live { animation: none; }
}

.wall { position: absolute; background: transparent; }
.hole-seal { position: absolute; border-radius: 10px; background: transparent; }

.tip {
  position: absolute;
  width: 268px;
  max-width: calc(100% - 20px);
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 10px;
  padding: 12px 14px 10px;
  box-shadow: 0 8px 26px color-mix(in srgb, var(--neu-shadow-dark) 70%, transparent),
              0 0 0 1px var(--border-color);
  transition: top .34s cubic-bezier(.4, 0, .2, 1), left .34s cubic-bezier(.4, 0, .2, 1);
}
.tip h4 { margin: 0 0 6px; font-size: 13.5px; font-weight: 600; }
.tip-list { list-style: none; margin: 0; padding: 0; counter-reset: tipitem; }
.tip-list li {
  counter-increment: tipitem;
  position: relative;
  padding-left: 16px;
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--text-secondary);
}
.tip-list li + li { margin-top: 3px; }
.tip-list li::before {
  content: counter(tipitem);
  position: absolute;
  left: 0;
  top: .5px;
  font-size: 11px;
  font-weight: 700;
  color: var(--tour-strong);
}
.tip .hint {
  display: block;
  margin-top: 8px;
  font-size: 11.5px;
  color: var(--tour-strong);
}
.tip .arrow {
  position: absolute;
  width: 10px;
  height: 10px;
  background: var(--bg-card);
  transform: rotate(45deg);
}
.tip[data-side="top"] .arrow { bottom: -4px; border-right: 1px solid var(--border-color); border-bottom: 1px solid var(--border-color); }
.tip[data-side="bottom"] .arrow { top: -4px; border-left: 1px solid var(--border-color); border-top: 1px solid var(--border-color); }
.tip[data-side="left"] .arrow { right: -4px; border-left: 1px solid var(--border-color); border-top: 1px solid var(--border-color); }
.tip[data-side="right"] .arrow { left: -4px; border-right: 1px solid var(--border-color); border-bottom: 1px solid var(--border-color); }

.tip-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 11px;
  padding-top: 9px;
  border-top: 1px solid var(--border-color);
}
.tip-progress { font-size: 11px; color: var(--text-muted); letter-spacing: .5px; }
.tip-btns { display: flex; gap: 6px; }
.tip-btn {
  border: none;
  border-radius: 7px;
  padding: 5px 12px;
  font-size: 12px;
  cursor: pointer;
  background: var(--neu-bg);
  color: var(--text-secondary);
  box-shadow: 2px 2px 4px var(--neu-shadow-dark), -2px -2px 4px var(--neu-shadow-light);
  transition: transform .18s cubic-bezier(.34, 1.56, .64, 1), box-shadow .18s, color .18s;
}
.tip-btn:hover { color: var(--text-primary); }
.tip-btn:not(.primary):active {
  transform: scale(.92);
  box-shadow: inset 2px 2px 4px var(--neu-shadow-dark), inset -2px -2px 4px var(--neu-shadow-light);
}
.tip-btn.primary {
  background: #6a8a8f;
  color: #fff;
  box-shadow: 0 2px 6px color-mix(in srgb, #6a8a8f 45%, transparent);
}
.tip-btn.primary:hover { background: #5a787d; color: #fff; }
.tip-btn.primary:active { transform: scale(.96); }
.tip-btn.ghost { background: transparent; box-shadow: none; color: var(--text-muted); }
.tip-btn.ghost:hover { color: var(--text-primary); }

.skip-all {
  /* 并入气泡按钮行，放在「上一步」左边；比按钮更安静，用分隔线与步进操作区分开 */
  border: none;
  background: transparent;
  color: var(--text-muted);
  font-size: 11.5px;
  cursor: pointer;
  padding: 5px 8px 5px 2px;
  margin-right: 2px;
  border-right: 1px solid var(--border-color);
  transition: color .18s;
}
.skip-all:hover { color: var(--text-primary); }

/* 居中卡片 */
.center-card {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--tour-scrim);
}
.center-card .card {
  width: 320px;
  max-width: calc(100% - 24px);
  max-height: calc(100% - 24px);
  overflow-y: auto;
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 14px;
  padding: 22px 24px 18px;
  box-shadow: 0 14px 40px rgba(0, 0, 0, .35), 0 0 0 1px var(--border-color);
  transform: scale(1);
  transition: transform .34s cubic-bezier(.34, 1.56, .64, 1);
}
.center-card h3 { margin: 0 0 4px; font-size: 18px; }
.center-card .sub { margin: 0 0 14px; font-size: 12.5px; color: var(--text-secondary); }
.center-card details { margin-bottom: 14px; font-size: 12.5px; color: var(--text-secondary); }
.center-card summary { cursor: pointer; color: var(--tour-strong); font-size: 12px; margin-bottom: 6px; }
.center-card ul { margin: 0; padding-left: 16px; line-height: 1.85; }
.row { display: flex; gap: 9px; align-items: flex-start; margin: 9px 0; font-size: 12.5px; }
.row .ico {
  width: 26px;
  height: 26px;
  border-radius: 8px;
  flex-shrink: 0;
  font-size: 13px;
  font-weight: 700;
  background: var(--neu-bg);
  color: var(--tour-strong);
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 2px 2px 4px var(--neu-shadow-dark), -2px -2px 4px var(--neu-shadow-light);
}
.row .tx b { display: block; font-size: 12.5px; font-weight: 600; margin-bottom: 1px; }
.row .tx span { color: var(--text-secondary); }
.card-btns { display: flex; gap: 8px; margin-top: 18px; }
.card-btns button { flex: 1; }

.tour-card-enter-active,
.tour-card-leave-active { transition: opacity .26s ease; }
.tour-card-enter-from,
.tour-card-leave-to { opacity: 0; }
.tour-card-enter-from .card { transform: scale(.94); }
</style>
