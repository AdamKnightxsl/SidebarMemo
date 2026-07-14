<script setup lang="ts">
const emit = defineEmits<{
  close: [];
}>();
</script>

<template>
  <div class="guide-overlay">
    <div class="guide-modal">
      <h2>欢迎使用 Sidebar Memo</h2>
      <p class="guide-subtitle">一个轻量级的侧边栏备忘录工具</p>

      <div class="guide-section">
        <h3>主要功能</h3>
        <ul>
          <li><strong>快速记录</strong> — 底部输入框输入内容，回车即保存</li>
          <li><strong>提醒</strong> — 点击铃铛图标设置提醒时间</li>
          <li><strong>颜色标记</strong> — 点击画笔图标给备忘录分类</li>
          <li><strong>置顶</strong> — 重要备忘录置顶显示</li>
          <li><strong>垃圾桶</strong> — 删除的备忘录可恢复，30天自动清理</li>
        </ul>
      </div>

      <div class="guide-section">
        <h3>快捷键</h3>
        <div class="shortcut-grid">
          <div class="shortcut-item">
            <kbd>Ctrl+N</kbd>
            <span>新建备忘录</span>
          </div>
          <div class="shortcut-item">
            <kbd>Ctrl+F</kbd>
            <span>搜索</span>
          </div>
          <div class="shortcut-item">
            <kbd>↑ ↓</kbd>
            <span>选择备忘录</span>
          </div>
          <div class="shortcut-item">
            <kbd>Enter</kbd>
            <span>编辑选中项</span>
          </div>
          <div class="shortcut-item">
            <kbd>Escape</kbd>
            <span>取消选择</span>
          </div>
        </div>
      </div>

      <div class="guide-section">
        <h3>窗口操作</h3>
        <ul>
          <li>拖拽边缘可调整窗口大小</li>
          <li>鼠标移至屏幕边缘可自动吸附</li>
          <li>全局快捷键（默认 Alt+M）可随时呼出窗口</li>
        </ul>
      </div>

      <button class="guide-btn" @click="emit('close')">知道了</button>
    </div>
  </div>
</template>

<style scoped>
.guide-overlay {
  position: fixed;
  inset: 0;
  background: var(--bg-main, #f5f5f5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10000;
  overflow: hidden;
}

.dark .guide-overlay {
  background: #1a1a2e;
}

.guide-modal {
  max-width: 420px;
  width: 90%;
  max-height: 90vh;
  overflow-y: auto;
  padding: 24px 28px;
  user-select: none;
}

.dark .guide-modal {
  color: #e0e0e0;
}

.guide-modal h2 {
  margin: 0 0 4px;
  font-size: 20px;
  color: var(--text-primary, #1a1a1a);
}

.guide-subtitle {
  margin: 0 0 16px;
  color: var(--text-secondary, #888);
  font-size: 13px;
}

.guide-section {
  margin-bottom: 16px;
}

.guide-section h3 {
  margin: 0 0 8px;
  font-size: 14px;
  color: var(--text-primary, #1a1a1a);
  border-bottom: 1px solid var(--border-color, #eee);
  padding-bottom: 6px;
}

.guide-section ul {
  margin: 0;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.8;
  color: var(--text-secondary, #555);
}

.shortcut-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}

.shortcut-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.shortcut-item kbd {
  background: var(--bg-hover, #f0f0f0);
  border: 1px solid var(--border-color, #ddd);
  border-radius: 4px;
  padding: 2px 6px;
  font-family: monospace;
  font-size: 12px;
  min-width: 32px;
  text-align: center;
}

.guide-btn {
  display: block;
  width: 100%;
  margin-top: 16px;
  padding: 10px;
  background: #3b82f6;
  color: #fff;
  border: none;
  border-radius: 8px;
  font-size: 14px;
  cursor: pointer;
  transition: background 0.2s;
}

.guide-btn:hover {
  background: #2563eb;
}
</style>
