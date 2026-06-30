<script setup lang="ts">
// Renders the shared toast queue. @gnas/ui-shared exposes useToast() but no
// container component, so each app provides its own.
import { computed } from 'vue'
import { useToast } from '@gnas/ui-shared'

const toast = useToast()
const items = computed(() => toast.toasts.value)
function dismiss(id: string) {
  toast.removeToast(id)
}
</script>

<template>
  <div class="toast-stack" aria-live="polite">
    <button
      v-for="t in items"
      :key="t.id"
      class="toast"
      :class="t.type"
      type="button"
      @click="dismiss(t.id)"
    >
      {{ t.message }}
    </button>
  </div>
</template>

<style scoped>
.toast-stack {
  position: fixed;
  bottom: 16px;
  right: 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  z-index: 1000;
  max-width: min(360px, calc(100vw - 32px));
}
.toast {
  text-align: left;
  padding: 10px 14px;
  border-radius: var(--app-radius, 0.5rem);
  font-size: 13px;
  line-height: 1.4;
  background: var(--app-surface, var(--color-surface));
  color: var(--app-fg, var(--color-text));
  border: 1px solid var(--app-border, var(--color-border));
  border-left-width: 3px;
  box-shadow: 0 6px 20px rgb(0 0 0 / 0.25);
  cursor: pointer;
}
.toast.success { border-left-color: var(--app-success, var(--color-success)); }
.toast.error { border-left-color: var(--app-danger, var(--color-danger)); }
.toast.warning { border-left-color: var(--app-warning, var(--color-warning)); }
.toast.info { border-left-color: var(--app-primary, var(--color-accent)); }
</style>
