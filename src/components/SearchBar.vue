<script setup lang="ts">
import { ref } from "vue";

const model = defineModel<string>({ default: "" });
const input = ref<HTMLInputElement | null>(null);

function focus() {
  input.value?.focus();
}

defineExpose({ focus });
</script>

<template>
  <div class="header">
    <div class="search-box">
      <input
        ref="input"
        v-model="model"
        type="text"
        placeholder="搜索备忘..."
      />
      <button v-if="model" class="search-clear" @click="model = ''">✕</button>
    </div>
  </div>
</template>
