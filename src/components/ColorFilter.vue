<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { useMemos } from "../composables/useMemos";
import { usePopupPosition } from "../composables/usePopupPosition";
import { useClickOutside } from "../composables/useClickOutside";

const COLORS = ["pink", "blue", "green", "yellow"];
const COLOR_NAMES: Record<string, string> = {
  pink: "粉色",
  blue: "蓝色",
  green: "绿色",
  yellow: "黄色",
};

const { colorFilter, colorCounts } = useMemos();
const btnRef = ref<HTMLButtonElement | null>(null);
const open = ref(false);
const { popupStyle, updatePosition } = usePopupPosition(btnRef);

useClickOutside({
  ignore: [".color-filter-popup", ".color-filter-btn"],
  onClickOutside: () => { open.value = false; },
});

const activeCount = computed(() => colorFilter.value.length);
const btnTitle = computed(() =>
  activeCount.value === 0
    ? "按标记颜色筛选"
    : "已筛选：" + colorFilter.value.map((c) => COLOR_NAMES[c] || c).join("、")
);

function togglePopup() {
  open.value = !open.value;
  if (open.value) nextTick(() => updatePosition());
}

function toggleColor(color: string) {
  const i = colorFilter.value.indexOf(color);
  if (i >= 0) colorFilter.value.splice(i, 1);
  else colorFilter.value.push(color);
}

function clearFilter() {
  colorFilter.value = [];
  open.value = false;
}
</script>

<template>
  <button
    ref="btnRef"
    class="color-filter-btn"
    :class="{ active: activeCount > 0, open }"
    :title="btnTitle"
    @click.stop="togglePopup"
  >
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3"/>
    </svg>
    <span v-if="activeCount > 0" class="color-filter-badge">{{ activeCount }}</span>
  </button>

  <Teleport to="body">
    <div v-if="open" class="color-filter-popup" :style="popupStyle" @click.stop>
      <div class="color-filter-head">按标记颜色筛选</div>
      <button
        v-for="c in COLORS"
        :key="c"
        class="color-filter-item"
        :class="{ active: colorFilter.includes(c) }"
        :disabled="!colorCounts[c] && !colorFilter.includes(c)"
        @click="toggleColor(c)"
      >
        <span class="color-dot" :data-c="c"></span>
        <span class="color-filter-name">{{ COLOR_NAMES[c] }}</span>
        <span class="color-filter-num">{{ colorCounts[c] || 0 }}</span>
      </button>
      <button class="color-filter-clear" :disabled="activeCount === 0" @click="clearFilter">清除筛选</button>
    </div>
  </Teleport>
</template>
