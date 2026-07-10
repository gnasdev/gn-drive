<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { CRON_PRESETS } from '@/constants/forms'

const model = defineModel<string>({ default: '' })

const props = withDefaults(
  defineProps<{
    testId?: string
    /** Allow empty schedule (flows optional cron). */
    allowNone?: boolean
    required?: boolean
  }>(),
  {
    testId: 'cron-field',
    allowNone: false,
    required: false,
  },
)

const { t } = useI18n()

const CUSTOM = '__custom__'
const NONE = '__none__'

const presetValues = CRON_PRESETS.map((p) => p.value)

const selectValue = ref(resolveSelect(model.value))
const customCron = ref(isCustom(model.value) ? model.value : '0 * * * *')

function isCustom(v: string) {
  const s = (v ?? '').trim()
  if (!s) return false
  return !presetValues.includes(s as (typeof presetValues)[number])
}

function resolveSelect(v: string) {
  const s = (v ?? '').trim()
  if (!s) return props.allowNone ? NONE : (CRON_PRESETS[0]?.value ?? CUSTOM)
  if (presetValues.includes(s as (typeof presetValues)[number])) return s
  return CUSTOM
}

watch(
  () => model.value,
  (v) => {
    selectValue.value = resolveSelect(v)
    if (isCustom(v)) customCron.value = v
  },
)

const showCustom = computed(() => selectValue.value === CUSTOM)

function emitFromUi() {
  if (selectValue.value === NONE) {
    model.value = ''
    return
  }
  if (selectValue.value === CUSTOM) {
    model.value = customCron.value.trim()
    return
  }
  model.value = selectValue.value
}

watch(selectValue, emitFromUi)
watch(customCron, () => {
  if (selectValue.value === CUSTOM) emitFromUi()
})
</script>

<template>
  <div class="flex flex-col gap-1.5">
    <select
      v-model="selectValue"
      class="field-input"
      :data-testid="testId"
      :required="required && selectValue !== NONE"
    >
      <option v-if="allowNone" :value="NONE">{{ t('cron.none') }}</option>
      <option v-for="p in CRON_PRESETS" :key="p.value" :value="p.value">
        {{ t(`cron.${p.key}`) }} ({{ p.value }})
      </option>
      <option :value="CUSTOM">{{ t('cron.custom') }}</option>
    </select>
    <input
      v-if="showCustom"
      v-model="customCron"
      type="text"
      class="field-input"
      :placeholder="t('cron.customPlaceholder')"
      :data-testid="`${testId}-custom`"
      :required="required"
    />
    <p class="m-0 text-[10px] text-text-dim">{{ t('cron.hint') }}</p>
  </div>
</template>
