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
      title="全部备忘"
    >
      📋
    </button>
    <button
      class="nav-btn"
      :class="{ active: current === 'today' }"
      @click="$emit('change', 'today')"
      title="今日"
    >
      📅
    </button>
    <button
      class="nav-btn"
      :class="{ active: current === 'yesterday' }"
      @click="$emit('change', 'yesterday')"
      title="昨日"
    >
      📆
    </button>
    <button
      class="nav-btn"
      :class="{ active: current === 'day_before_yesterday' }"
      @click="$emit('change', 'day_before_yesterday')"
      title="前日"
    >
      📅
    </button>
    <button
      class="nav-btn"
      :class="{ active: current === 'trash' }"
      @click="$emit('change', 'trash')"
      title="垃圾桶"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <polyline points="3 6 5 6 21 6"/>
        <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
        <path d="M10 11v6"/>
        <path d="M14 11v6"/>
        <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
      </svg>
    </button>
    <div class="nav-spacer"></div>
    <button
      class="nav-btn nav-btn-settings"
      :class="{ active: current === 'settings' }"
      @click="$emit('change', 'settings')"
      title="设置"
    >
      ⚙
    </button>
  </div>
</template>
