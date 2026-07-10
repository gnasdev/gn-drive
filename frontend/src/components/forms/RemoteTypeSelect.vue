<script setup lang="ts">
import { REMOTE_TYPES } from '@/constants/forms'

const model = defineModel<string>({ default: 'local' })

defineProps<{
  testId?: string
  disabled?: boolean
}>()
</script>

<template>
  <select
    v-model="model"
    class="field-input"
    :data-testid="testId || 'remote-type'"
    :disabled="disabled"
  >
    <option v-for="t in REMOTE_TYPES" :key="t" :value="t">{{ t }}</option>
  </select>
</template>
