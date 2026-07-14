<script setup lang="ts">
import { ref, nextTick } from "vue";

const emit = defineEmits<{
  submit: [content: string];
}>();

const text = ref("");
const textarea = ref<HTMLTextAreaElement | null>(null);

function autoResize() {
  if (!textarea.value) return;
  if (!text.value) {
    textarea.value.style.height = "";
    textarea.value.classList.remove("has-overflow");
    return;
  }
  textarea.value.style.height = "auto";
  textarea.value.style.height = textarea.value.scrollHeight + "px";
  textarea.value.classList.toggle("has-overflow", textarea.value.scrollHeight > textarea.value.clientHeight);
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    submit();
  }
}

function submit() {
  const v = text.value.trim();
  if (v) {
    emit("submit", v);
    text.value = "";
    nextTick(() => autoResize());
  }
}

function focus() {
  textarea.value?.focus();
}

defineExpose({ focus });
</script>

<template>
  <div class="quick-input-area">
    <div class="quick-input-wrap">
      <textarea
        ref="textarea"
        v-model="text"
        class="quick-input"
        placeholder="快速输入   Shift+Enter 换行"
        rows="1"
        @input="autoResize"
        @keydown="handleKeydown"
      ></textarea>
      <button class="send-btn" @click="submit" :disabled="!text.trim()">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 2L11 13" />
          <path d="M22 2L15 22L11 13L2 9L22 2Z" />
        </svg>
      </button>
    </div>
  </div>
</template>
