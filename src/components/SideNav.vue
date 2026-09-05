<script setup lang="ts">
defineProps<{
  current: string;
}>();

defineEmits<{
  change: [view: "memos" | "today" | "yesterday" | "day_before_yesterday" | "trash" | "settings"];
}>();
</script>

<template>
  <div class="side-nav">
    <button
      class="nav-btn"
      :class="{ active: current === 'memos' }"
      @click="$emit('change', 'memos')"
      data-nav="memos"
      title="全部备忘"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>
        <rect x="8" y="2" width="8" height="4" rx="1" ry="1"/>
        <line x1="9" y1="12" x2="15" y2="12"/>
        <line x1="9" y1="16" x2="13" y2="16"/>
      </svg>
    </button>
    <button
      class="nav-btn nav-btn-char"
      :class="{ active: current === 'today' }"
      @click="$emit('change', 'today')"
      data-nav="today"
      title="今日"
    >
      今
    </button>
    <button
      class="nav-btn nav-btn-char"
      :class="{ active: current === 'yesterday' }"
      @click="$emit('change', 'yesterday')"
      data-nav="yesterday"
      title="昨日"
    >
      昨
    </button>
    <button
      class="nav-btn nav-btn-char"
      :class="{ active: current === 'day_before_yesterday' }"
      @click="$emit('change', 'day_before_yesterday')"
      data-nav="dby"
      title="前日"
    >
      前
    </button>
    <button
      class="nav-btn"
      :class="{ active: current === 'trash' }"
      @click="$emit('change', 'trash')"
      data-nav="trash"
      title="垃圾桶"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="3 6 5 6 21 6"/>
        <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
        <path d="M10 11v6"/>
        <path d="M14 11v6"/>
        <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
      </svg>
    </button>
    <div class="nav-spacer"></div>
    <button
      class="nav-btn"
      :class="{ active: current === 'settings' }"
      @click="$emit('change', 'settings')"
      data-nav="settings"
      title="设置"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="3"/>
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
      </svg>
    </button>
  </div>
</template>
