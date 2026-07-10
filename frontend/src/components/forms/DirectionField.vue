<script setup lang="ts">
/**
 * Profile direction select with inline help for the active option
 * (push / pull / bi / bi-resync).
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { SYNC_ACTIONS, type SyncAction } from '@/constants/forms'

const model = defineModel<string>({ default: 'push' })

defineProps<{
  disabled?: boolean
  testId?: string
  /** Highlight invalid state from parent validator */
  invalid?: boolean
}>()

const emit = defineEmits<{
  focus: []
}>()

const { t, te } = useI18n()

const selected = computed(() => (model.value || 'push') as SyncAction)

const helpTitle = computed(() => {
  const key = `syncHelp.${selected.value}.title`
  return te(key) ? t(key) : ''
})

const helpBody = computed(() => {
  const key = `syncHelp.${selected.value}.body`
  return te(key) ? t(key) : ''
})

function optionLabel(a: SyncAction): string {
  const key = `profiles.directionOptions.${a}`
  return te(key) ? t(key) : a
}
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <select
      v-model="model"
      class="field-input"
      :class="invalid && 'border-danger'"
      :disabled="disabled"
      :data-testid="testId || 'profiles-direction'"
      @focus="emit('focus')"
    >
      <option v-for="a in SYNC_ACTIONS" :key="a" :value="a">
        {{ optionLabel(a) }}
      </option>
    </select>

    <div
      v-if="helpTitle || helpBody"
      class="border-2 border-border bg-bg-secondary px-3 py-2 shadow-[var(--shadow-neo-sm)]"
      data-testid="profiles-direction-help"
    >
      <div class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
        <span class="font-mono text-[12px] font-bold text-text">{{ selected }}</span>
        <span v-if="helpTitle" class="text-[12px] font-bold text-text">{{ helpTitle }}</span>
      </div>
      <p v-if="helpBody" class="m-0 mt-1 text-[11px] leading-relaxed text-text-muted">
        {{ helpBody }}
      </p>
    </div>
  </div>
</template>
