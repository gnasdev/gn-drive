<script setup lang="ts">
/**
 * NeoBrutalism checkbox — Solarized desktop v0.4 style.
 * Square 2px border, hard shadow, bold label; no native rounded checkbox chrome.
 */
const model = defineModel<boolean>({ default: false })

defineProps<{
  label?: string
  disabled?: boolean
  /** Optional data-testid for e2e */
  testId?: string
}>()
</script>

<template>
  <label
    class="group inline-flex cursor-pointer select-none items-center gap-2.5 text-[13px] font-bold text-text"
    :class="disabled && 'cursor-not-allowed opacity-50'"
  >
    <input
      v-model="model"
      type="checkbox"
      class="peer sr-only"
      :disabled="disabled"
      :data-testid="testId"
    />
    <span
      class="relative inline-flex h-5 w-5 shrink-0 items-center justify-center border-2 border-border text-text transition-all duration-100 peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-accent-strong"
      :class="[
        model ? 'bg-accent' : 'bg-bg group-hover:bg-surface-hover',
        disabled ? 'shadow-none' : 'shadow-[var(--shadow-neo-sm)]',
        !disabled && 'group-active:translate-y-px group-active:shadow-none',
      ]"
      aria-hidden="true"
    >
      <svg
        v-if="model"
        class="h-3 w-3"
        viewBox="0 0 12 12"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="square"
        stroke-linejoin="miter"
      >
        <path d="M2 6.5 L4.5 9 L10 3" />
      </svg>
    </span>
    <span v-if="label" class="leading-tight">{{ label }}</span>
    <span v-else class="leading-tight"><slot /></span>
  </label>
</template>
