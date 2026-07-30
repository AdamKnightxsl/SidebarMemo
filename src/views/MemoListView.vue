<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick, watchEffect, provide } from "vue";
import SearchBar from "../components/SearchBar.vue";
import MemoCard from "../components/MemoCard.vue";
import QuickInput from "../components/QuickInput.vue";
import { useMemos } from "../composables/useMemos";
import { useSettings } from "../composables/useSettings";
import { marked } from "marked";
import { matchesQuery, highlightInHtml } from "../composables/pinyinSearch";
import { sanitizeHtml } from "../composables/sanitizeHtml";
import { isComposing } from "../utils";
import { selectedIndex } from "../composables/useKeyboard";

const { pinnedMemos, unpinnedMemos, addMemo, searchQuery, dateFilter, trashedMemos, loadTrashedMemos, restoreFromTrash, permanentDeleteMemo, clearTrash } = useMemos();
const { settings } = useSettings();

// 向 MemoCard 提供 searchQuery，用于搜索高亮
provide('searchQuery', searchQuery);

const trashSearch = ref("");
const searchRef = ref<InstanceType<typeof SearchBar> | null>(null);
const inputRef = ref<InstanceType<typeof QuickInput> | null>(null);
const normalListRef = ref<HTMLElement | null>(null);
const trashListRef = ref<HTMLElement | null>(null);
const normalTrackRef = ref<HTMLElement | null>(null);
const trashTrackRef = ref<HTMLElement | null>(null);
const normalThumbRef = ref<HTMLElement | null>(null);
const trashThumbRef = ref<HTMLElement | null>(null);

const filteredTrashed = computed(() => {
  const q = trashSearch.value.trim();
  if (!q) return trashedMemos.value;
  return trashedMemos.value.filter((m) => matchesQuery(m.content, q));
});

function renderTrashContent(content: string): string {
  const html = marked.parse(content || '') as string;
  const q = trashSearch.value.trim();
  const finalHtml = q ? highlightInHtml(html, q) : html;
  // 回收站内容同样经 v-html 注入，必须净化
  return sanitizeHtml(finalHtml);
}

// ── 自定义滚动条 ──
let scrollbarHideTimer: ReturnType<typeof setTimeout> | null = null;
let _scrollbarUpdateThumb: (() => void) | null = null;
let _scrollbarListEl: HTMLElement | null = null;

function cleanupScrollbar() {
  if (_scrollbarUpdateThumb) {
    _scrollbarListEl?.removeEventListener("scroll", _scrollbarUpdateThumb);
    _scrollbarUpdateThumb = null;
    _scrollbarListEl = null;
  }
}

function setupScrollbar(list: HTMLElement, track: HTMLElement, thumb: HTMLElement) {
  cleanupScrollbar();

  function updateThumb() {
    const { scrollTop, scrollHeight, clientHeight } = list;
    if (scrollHeight <= clientHeight) {
      track.classList.remove("visible");
      return;
    }
    const ratio = clientHeight / scrollHeight;
    const thumbH = Math.max(20, clientHeight * ratio);
    const thumbTop = (scrollTop / (scrollHeight - clientHeight)) * (clientHeight - thumbH);
    thumb.style.height = thumbH + "px";
    thumb.style.top = thumbTop + "px";
    track.classList.add("visible");
    if (scrollbarHideTimer) clearTimeout(scrollbarHideTimer);
    scrollbarHideTimer = setTimeout(() => {
      track.classList.remove("visible");
    }, 800);
  }

  _scrollbarUpdateThumb = updateThumb;
  _scrollbarListEl = list;
  list.addEventListener("scroll", updateThumb);
  requestAnimationFrame(updateThumb);
}

const trashedDeletingId = ref<string | null>(null);
function handlePermanentDelete(id: string) {
  trashedDeletingId.value = id;
  setTimeout(() => {
    trashedDeletingId.value = null;
    permanentDeleteMemo(id);
  }, 250);
}

const allMemos = computed(() => [...pinnedMemos.value, ...unpinnedMemos.value]);
const maxIndex = computed(() => allMemos.value.length - 1);

async function handleAdd(content: string) {
  if (!content.trim()) return;
  await addMemo(content.trim());
  selectedIndex.value = -1;
}

function moveSelection(delta: number) {
  if (allMemos.value.length === 0) return;
  const next = selectedIndex.value + delta;
  if (next < 0) selectedIndex.value = 0;
  else if (next > maxIndex.value) selectedIndex.value = maxIndex.value;
  else selectedIndex.value = next;
  scrollSelectedIntoView();
}

function scrollSelectedIntoView() {
  const id = allMemos.value[selectedIndex.value]?.id;
  if (!id) return;
  const el = document.querySelector(`.memo-card[data-memo-id="${id}"]`);
  el?.scrollIntoView({ block: "nearest" });
}

function editSelected() {
  const id = allMemos.value[selectedIndex.value]?.id;
  if (!id) return;
  const el = document.querySelector(`.memo-card[data-memo-id="${id}"]`);
  el?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
}

function handleKeydown(e: KeyboardEvent) {
  // 输入法组词中的按键不参与列表快捷键导航
  if (isComposing(e)) return;
  const tag = (e.target as HTMLElement).tagName;
  const isEditing = tag === "TEXTAREA" || tag === "INPUT";

  if ((e.ctrlKey || e.metaKey) && (e.key === "n" || e.key === "N")) {
    e.preventDefault();
    inputRef.value?.focus();
  } else if ((e.ctrlKey || e.metaKey) && (e.key === "f" || e.key === "F")) {
    e.preventDefault();
    searchRef.value?.focus();
  } else if (!isEditing) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveSelection(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveSelection(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      editSelected();
    } else if (e.key === "Escape") {
      selectedIndex.value = -1;
    }
  }
}

onMounted(() => {
  document.addEventListener("keydown", handleKeydown);
});

// ── 滚动入场动画（scroll 事件驱动，不依赖 IntersectionObserver） ──
let scrollCleanupFn: (() => void) | null = null;
let checkVisibilityRef: (() => void) | null = null;
let scrollAnimationTimer: ReturnType<typeof setTimeout> | null = null;

watchEffect(() => {
  const list = normalListRef.value || trashListRef.value;
  if (!list) return;
  setupScrollAnimation(list);
  const track = normalTrackRef.value || trashTrackRef.value;
  const thumb = normalThumbRef.value || trashThumbRef.value;
  if (track && thumb) setupScrollbar(list, track, thumb);
});

function setupScrollAnimation(list: HTMLElement) {
  scrollCleanupFn?.();

  const wrapper = list.parentElement;
  const topGrad = wrapper?.querySelector(".memo-list-gradient-top") as HTMLElement;
  const bottomGrad = wrapper?.querySelector(".memo-list-gradient-bottom") as HTMLElement;
  if (topGrad) topGrad.style.opacity = "0";

  function checkVisibility() {
    list.classList.add("is-scroll-animating");
    if (scrollAnimationTimer) clearTimeout(scrollAnimationTimer);
    scrollAnimationTimer = setTimeout(() => {
      list.classList.remove("is-scroll-animating");
      scrollAnimationTimer = null;
    }, 260);

    const cards = list.querySelectorAll(".memo-card");
    const listRect = list.getBoundingClientRect();
    cards.forEach((card) => {
      if (card.classList.contains("drag-placeholder") || card.classList.contains("drag-over")) return;
      const el = card as HTMLElement;
      const cardRect = el.getBoundingClientRect();
      const visible = cardRect.bottom > listRect.top && cardRect.top < listRect.bottom;
      el.style.opacity = visible ? "1" : "0";
      el.style.transform = visible ? "" : "scale(0.7)";
    });

    const { scrollTop, scrollHeight, clientHeight } = list;
    if (topGrad) topGrad.style.opacity = String(Math.min(scrollTop / 50, 1));
    const bottomDist = scrollHeight - (scrollTop + clientHeight);
    if (bottomGrad) bottomGrad.style.opacity = scrollHeight <= clientHeight ? "0" : String(Math.min(bottomDist / 50, 1));
  }

  list.addEventListener("scroll", checkVisibility);
  window.addEventListener("resize", checkVisibility);
  requestAnimationFrame(checkVisibility);
  checkVisibilityRef = checkVisibility;

  scrollCleanupFn = () => {
    list.removeEventListener("scroll", checkVisibility);
    window.removeEventListener("resize", checkVisibility);
    if (scrollAnimationTimer) {
      clearTimeout(scrollAnimationTimer);
      scrollAnimationTimer = null;
    }
    list.classList.remove("is-scroll-animating");
  };
}

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeydown);
  scrollCleanupFn?.();
  cleanupScrollbar();
  if (scrollbarHideTimer) clearTimeout(scrollbarHideTimer);
});

function scrollToMemo(id: string) {
  const el = document.querySelector(`.memo-card[data-memo-id="${id}"]`);
  if (!el) return;
  el.scrollIntoView({ block: "nearest", behavior: "smooth" });
  el.classList.remove("reminder-flash");
  void (el as HTMLElement).offsetWidth;
  el.classList.add("reminder-flash");
}

defineExpose({ scrollToMemo });

watch(dateFilter, (v) => {
  if (v === "trash") {
    loadTrashedMemos();
  }
  selectedIndex.value = -1;
}, { immediate: true });

watch([pinnedMemos, unpinnedMemos], () => {
  if (selectedIndex.value > maxIndex.value) {
    selectedIndex.value = maxIndex.value;
  }
  nextTick(() => requestAnimationFrame(() => checkVisibilityRef?.()));
});

watch(trashedMemos, () => {
  nextTick(() => requestAnimationFrame(() => checkVisibilityRef?.()));
});

watch(() => settings.value.skin, () => {
  nextTick(() => requestAnimationFrame(() => checkVisibilityRef?.()));
});
</script>

<template>
  <!-- 垃圾桶视图 -->
  <template v-if="dateFilter === 'trash'">
    <SearchBar v-model="trashSearch" :count="filteredTrashed.length" />
    <div class="memo-list-wrapper">
      <div class="memo-list-gradient-top"></div>
      <div class="memo-list-gradient-bottom"></div>
      <div class="memo-list scrollbar-hide" ref="trashListRef">
        <div class="shadow-spacer"></div>
        <template v-if="filteredTrashed.length > 0">
          <div
            v-for="memo in filteredTrashed"
            :key="memo.id"
            class="memo-card"
            :class="{ deleting: trashedDeletingId === memo.id }"
            :data-color="memo.color || undefined"
          >
            <div class="memo-header">
              <span class="memo-time">
                <span class="memo-date">{{ memo.created_at.slice(0, 10) }}</span>
                <span class="memo-clock">{{ memo.created_at.slice(11, 16) }}</span>
              </span>
              <div class="memo-actions" style="opacity: 1;">
                <button
                  class="memo-action-btn"
                  @click.stop="restoreFromTrash(memo.id)"
                  title="恢复"
                >↩</button>
                <button
                  class="memo-action-btn delete-btn"
                  @click.stop="handlePermanentDelete(memo.id)"
                  title="永久删除"
                ><span style="font-size: 20px;">×</span></button>
              </div>
            </div>
            <div class="memo-content markdown-body" style="max-height: none; -webkit-line-clamp: unset; display: block; overflow: visible;" v-html="renderTrashContent(memo.content)"></div>
          </div>
        </template>
        <div v-if="trashedMemos.length === 0" class="empty-state">
          <div class="empty-icon">🗑️</div>
          <div class="empty-text">垃圾桶是空的</div>
        </div>
      </div>
      <div class="memo-scroll-track" ref="trashTrackRef">
        <div class="memo-scroll-thumb" ref="trashThumbRef"></div>
      </div>
    </div>
    <div v-if="trashedMemos.length > 0" style="padding: 8px 12px; display: flex; justify-content: flex-end;">
      <button class="clear-trash-btn" @click="clearTrash" title="清空垃圾桶">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="3 6 5 6 21 6"/>
          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
          <path d="M10 11v6"/>
          <path d="M14 11v6"/>
          <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
        </svg>
        <span>清空</span>
      </button>
    </div>
  </template>

  <!-- 正常视图 -->
  <template v-else>
    <SearchBar ref="searchRef" v-model="searchQuery" :count="allMemos.length" />
    <div class="memo-list-wrapper">
      <div class="memo-list-gradient-top"></div>
      <div class="memo-list-gradient-bottom"></div>
      <div class="memo-list scrollbar-hide" ref="normalListRef">
        <div class="shadow-spacer"></div>
        <template v-if="pinnedMemos.length > 0">
          <MemoCard
            v-for="(memo, idx) in pinnedMemos"
            :key="memo.id"
            :memo="memo"
            :selected="selectedIndex === idx"
          />
          <div class="pinned-divider">
            <span class="pinned-divider-line"></span>
            <span class="pinned-divider-text">置顶</span>
            <span class="pinned-divider-line"></span>
          </div>
        </template>
        <template v-if="unpinnedMemos.length > 0">
          <MemoCard
            v-for="(memo, idx) in unpinnedMemos"
            :key="memo.id"
            :memo="memo"
            :selected="selectedIndex === pinnedMemos.length + idx"
          />
        </template>
        <div v-if="pinnedMemos.length === 0 && unpinnedMemos.length === 0" class="empty-state">
          <div class="empty-icon">📝</div>
          <div class="empty-text">输入内容开始记录</div>
        </div>
      </div>
      <div class="memo-scroll-track" ref="normalTrackRef">
        <div class="memo-scroll-thumb" ref="normalThumbRef"></div>
      </div>
    </div>
    <QuickInput ref="inputRef" @submit="handleAdd" />
  </template>
</template>
