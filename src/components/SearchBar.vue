<script setup lang="ts">
import { ref } from "vue";

const model = defineModel<string>({ default: "" });
const props = defineProps<{ count?: number }>();
const input = ref<HTMLInputElement | null>(null);

function focus() {
  input.value?.focus();
}

defineExpose({ focus });
</script>

<template>
  <div class="header">
    <div class="search-box" data-tour="search-box">
      <input
        ref="input"
        v-model="model"
        type="text"
        placeholder="搜索备忘..."
      />
      <span v-if="props.count !== undefined" class="search-count">{{ props.count }} 条</span>
      <button v-if="model" class="search-clear" @click="model = ''">✕</button>
    </div>
    <slot name="actions" />
  </div>
</template>
