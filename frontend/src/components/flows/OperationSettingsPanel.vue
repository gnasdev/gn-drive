<script setup lang="ts">
/**
 * Wails `operation-settings-panel` port: Performance / Filtering / Safety /
 * Comparison / Sync|Bisync options for a flow operation.
 * Schedule stays on the flow card (not per-op).
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhPlus, PhX, PhCaretDown } from '@phosphor-icons/vue'
import { FLOW_ACTIONS, type FlowAction } from '@/constants/forms'
import {
  type SyncConfig,
  parseSyncConfig,
  serializeSyncConfig,
} from '@/lib/syncConfig'
import AppCheckbox from '@/components/ui/Checkbox.vue'

const props = withDefaults(
  defineProps<{
    /** Raw operation.sync_config */
    modelValue: Record<string, unknown> | null | undefined
    /** Column action fallback */
    action?: string
    disabled?: boolean
  }>(),
  { disabled: false },
)

const emit = defineEmits<{
  'update:modelValue': [value: Record<string, unknown>]
  /** When action changes, parent should sync op.action column */
  'update:action': [action: FlowAction]
}>()

const { t } = useI18n()

const cfg = ref<SyncConfig>(parseSyncConfig(props.modelValue, props.action || 'push'))

// Path list drafts
const includeDrafts = ref<string[]>([])
const excludeDrafts = ref<string[]>([])

// Size/age split UI
const minSizeNum = ref('')
const minSizeUnit = ref('M')
const maxSizeNum = ref('')
const maxSizeUnit = ref('G')
const minAgeNum = ref('')
const minAgeUnit = ref('h')
const maxAgeNum = ref('')
const maxAgeUnit = ref('d')

const sizeUnits = [
  { value: 'k', label: 'KB' },
  { value: 'M', label: 'MB' },
  { value: 'G', label: 'GB' },
  { value: 'T', label: 'TB' },
]
const ageUnits = [
  { value: 's', label: 's' },
  { value: 'm', label: 'm' },
  { value: 'h', label: 'h' },
  { value: 'd', label: 'd' },
  { value: 'w', label: 'w' },
  { value: 'M', label: 'M' },
  { value: 'y', label: 'y' },
]

const conflictOptions = [
  { value: 'newer', labelKey: 'workspace.opSettings.conflictNewer' },
  { value: 'older', labelKey: 'workspace.opSettings.conflictOlder' },
  { value: 'larger', labelKey: 'workspace.opSettings.conflictLarger' },
  { value: 'smaller', labelKey: 'workspace.opSettings.conflictSmaller' },
  { value: 'path1', labelKey: 'workspace.opSettings.conflictSource' },
  { value: 'path2', labelKey: 'workspace.opSettings.conflictTarget' },
]
const conflictLoserOptions = [
  { value: 'delete', labelKey: 'workspace.opSettings.loserDelete' },
  { value: 'num', labelKey: 'workspace.opSettings.loserNum' },
  { value: 'pathname', labelKey: 'workspace.opSettings.loserPath' },
]
const deleteTimingOptions = [
  { value: '', labelKey: 'workspace.opSettings.deleteDefault' },
  { value: 'before', labelKey: 'workspace.opSettings.deleteBefore' },
  { value: 'during', labelKey: 'workspace.opSettings.deleteDuring' },
  { value: 'after', labelKey: 'workspace.opSettings.deleteAfter' },
]

const isBi = computed(
  () => cfg.value.action === 'bi' || cfg.value.action === 'bi-resync',
)
const isPush = computed(() => cfg.value.action === 'push')

function splitSizeAge(val: string | undefined): { n: string; u: string } {
  if (!val) return { n: '', u: 'M' }
  const m = /^([0-9.]+)\s*([a-zA-Z]+)?$/.exec(val.trim())
  if (!m) return { n: val, u: 'M' }
  return { n: m[1], u: m[2] || 'M' }
}

function joinSizeAge(n: string, u: string): string | undefined {
  const t = n.trim()
  if (!t || Number(t) === 0) return undefined
  return `${t}${u}`
}

function hydrateFromProps() {
  cfg.value = parseSyncConfig(props.modelValue, props.action || 'push')
  includeDrafts.value = [...(cfg.value.includedPaths ?? [])]
  excludeDrafts.value = [...(cfg.value.excludedPaths ?? [])]
  const minS = splitSizeAge(cfg.value.minSize)
  minSizeNum.value = minS.n
  minSizeUnit.value = minS.u === 'k' || minS.u === 'M' || minS.u === 'G' || minS.u === 'T' ? minS.u : 'M'
  const maxS = splitSizeAge(cfg.value.maxSize)
  maxSizeNum.value = maxS.n
  maxSizeUnit.value = maxS.u === 'k' || maxS.u === 'M' || maxS.u === 'G' || maxS.u === 'T' ? maxS.u : 'G'
  const minA = splitSizeAge(cfg.value.minAge)
  minAgeNum.value = minA.n
  minAgeUnit.value = minA.u || 'h'
  const maxA = splitSizeAge(cfg.value.maxAge)
  maxAgeNum.value = maxA.n
  maxAgeUnit.value = maxA.u || 'd'
}

watch(
  () => [props.modelValue, props.action] as const,
  () => hydrateFromProps(),
  { immediate: true, deep: true },
)

function emitConfig() {
  cfg.value.minSize = joinSizeAge(minSizeNum.value, minSizeUnit.value)
  cfg.value.maxSize = joinSizeAge(maxSizeNum.value, maxSizeUnit.value)
  cfg.value.minAge = joinSizeAge(minAgeNum.value, minAgeUnit.value)
  cfg.value.maxAge = joinSizeAge(maxAgeNum.value, maxAgeUnit.value)
  cfg.value.includedPaths = includeDrafts.value.map((s) => s.trim()).filter(Boolean)
  cfg.value.excludedPaths = excludeDrafts.value.map((s) => s.trim()).filter(Boolean)
  const serialized = serializeSyncConfig(cfg.value)
  emit('update:modelValue', serialized)
  emit('update:action', cfg.value.action)
}

function setAction(a: string) {
  cfg.value.action = a as FlowAction
  emitConfig()
}

function patchNum(key: keyof SyncConfig, raw: string) {
  const bag = cfg.value as unknown as Record<string, unknown>
  const t = raw.trim()
  if (t === '') {
    delete bag[key as string]
  } else {
    const n = Number(t)
    if (!Number.isNaN(n)) bag[key as string] = n
  }
  emitConfig()
}

function patchStr(key: keyof SyncConfig, raw: string) {
  const bag = cfg.value as unknown as Record<string, unknown>
  const t = raw.trim()
  if (t === '') delete bag[key as string]
  else bag[key as string] = t
  emitConfig()
}

function patchBool(key: keyof SyncConfig, v: boolean) {
  const bag = cfg.value as unknown as Record<string, unknown>
  bag[key as string] = v
  emitConfig()
}

function addInclude() {
  includeDrafts.value = [...includeDrafts.value, '']
}
function addExclude() {
  excludeDrafts.value = [...excludeDrafts.value, '']
}
function removeInclude(i: number) {
  includeDrafts.value = includeDrafts.value.filter((_, idx) => idx !== i)
  emitConfig()
}
function removeExclude(i: number) {
  excludeDrafts.value = excludeDrafts.value.filter((_, idx) => idx !== i)
  emitConfig()
}
</script>

<template>
  <div
    class="space-y-3 border-t-2 border-border bg-bg p-3"
    :class="disabled && 'pointer-events-none opacity-50'"
    data-testid="op-settings-panel"
  >
    <!-- Action + Dry run -->
    <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:items-end">
      <label class="field-label !mb-0">
        <span>{{ t('workspace.action') }}</span>
        <select
          class="field-input"
          :value="cfg.action"
          :disabled="disabled"
          data-testid="op-settings-action"
          @change="setAction(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="a in FLOW_ACTIONS" :key="a" :value="a">
            {{ t(`workspace.actionOptions.${a}`) }}
          </option>
        </select>
        <p class="m-0 mt-1 text-[11px] leading-relaxed text-text-muted">
          {{ t(`workspace.actionHelp.${cfg.action}`) }}
        </p>
      </label>
      <div class="flex items-end pb-1">
        <AppCheckbox
          :model-value="!!cfg.dryRun"
          :disabled="disabled"
          :label="t('workspace.dryRun')"
          test-id="op-settings-dry-run"
          @update:model-value="patchBool('dryRun', !!$event)"
        />
      </div>
    </div>

    <!-- Performance -->
    <section class="border-2 border-border">
      <div
        class="flex items-center gap-2 border-b-2 border-border border-l-4 border-l-[var(--color-info,#268bd2)] bg-bg-secondary px-3 py-2"
      >
        <span class="text-xs font-bold uppercase tracking-wide">{{
          t('workspace.opSettings.performance')
        }}</span>
      </div>
      <div class="space-y-3 p-3">
        <div class="grid grid-cols-2 gap-2">
          <label class="field-label !mb-0">
            <span>{{ t('workspace.opSettings.parallel') }}</span>
            <input
              class="field-input"
              type="number"
              min="0"
              placeholder="8"
              :value="cfg.parallel ?? ''"
              :disabled="disabled"
              @change="patchNum('parallel', ($event.target as HTMLInputElement).value)"
            />
          </label>
          <label class="field-label !mb-0">
            <span>{{ t('workspace.opSettings.bandwidth') }}</span>
            <input
              class="field-input"
              type="number"
              min="0"
              placeholder="0"
              :value="cfg.bandwidth ?? ''"
              :disabled="disabled"
              @change="patchNum('bandwidth', ($event.target as HTMLInputElement).value)"
            />
          </label>
        </div>
        <details class="group">
          <summary
            class="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-text-muted hover:text-text"
          >
            <PhCaretDown :size="12" class="transition-transform group-open:rotate-180" />
            {{ t('workspace.opSettings.advanced') }}
          </summary>
          <div class="mt-3 grid grid-cols-2 gap-2">
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.multiThread') }}</span>
              <input
                class="field-input"
                type="number"
                :value="cfg.multiThreadStreams ?? ''"
                :disabled="disabled"
                @change="patchNum('multiThreadStreams', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.bufferSize') }}</span>
              <input
                class="field-input"
                placeholder="16M"
                :value="cfg.bufferSize ?? ''"
                :disabled="disabled"
                @change="patchStr('bufferSize', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.retries') }}</span>
              <input
                class="field-input"
                type="number"
                :value="cfg.retries ?? ''"
                :disabled="disabled"
                @change="patchNum('retries', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.lowLevelRetries') }}</span>
              <input
                class="field-input"
                type="number"
                :value="cfg.lowLevelRetries ?? ''"
                :disabled="disabled"
                @change="patchNum('lowLevelRetries', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.maxDuration') }}</span>
              <input
                class="field-input"
                placeholder="1h30m"
                :value="cfg.maxDuration ?? ''"
                :disabled="disabled"
                @change="patchStr('maxDuration', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.retriesSleep') }}</span>
              <input
                class="field-input"
                placeholder="10s"
                :value="cfg.retriesSleep ?? ''"
                :disabled="disabled"
                @change="patchStr('retriesSleep', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.tpsLimit') }}</span>
              <input
                class="field-input"
                type="number"
                :value="cfg.tpsLimit ?? ''"
                :disabled="disabled"
                @change="patchNum('tpsLimit', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.connTimeout') }}</span>
              <input
                class="field-input"
                placeholder="30s"
                :value="cfg.connTimeout ?? ''"
                :disabled="disabled"
                @change="patchStr('connTimeout', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.ioTimeout') }}</span>
              <input
                class="field-input"
                placeholder="5m"
                :value="cfg.ioTimeout ?? ''"
                :disabled="disabled"
                @change="patchStr('ioTimeout', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.orderBy') }}</span>
              <input
                class="field-input"
                placeholder="size,desc"
                :value="cfg.orderBy ?? ''"
                :disabled="disabled"
                @change="patchStr('orderBy', ($event.target as HTMLInputElement).value)"
              />
            </label>
          </div>
          <div class="mt-2">
            <AppCheckbox
              :model-value="!!cfg.checkFirst"
              :disabled="disabled"
              :label="t('workspace.opSettings.checkFirst')"
              @update:model-value="patchBool('checkFirst', !!$event)"
            />
          </div>
        </details>
      </div>
    </section>

    <!-- Filtering -->
    <section class="border-2 border-border">
      <div
        class="flex items-center gap-2 border-b-2 border-border border-l-4 border-l-[var(--color-accent-strong,#6c71c4)] bg-bg-secondary px-3 py-2"
      >
        <span class="text-xs font-bold uppercase tracking-wide">{{
          t('workspace.opSettings.filtering')
        }}</span>
      </div>
      <div class="space-y-3 p-3">
        <div>
          <div class="mb-1 text-xs font-bold text-text-muted">{{ t('workspace.opSettings.includePaths') }}</div>
          <div
            v-for="(row, i) in includeDrafts"
            :key="`inc-${i}`"
            class="mb-1 flex items-center gap-2"
          >
            <input
              v-model="includeDrafts[i]"
              class="field-input font-mono text-sm"
              placeholder="/path or *.ext"
              :disabled="disabled"
              @change="emitConfig"
            />
            <button type="button" class="btn-ghost !px-1" :disabled="disabled" @click="removeInclude(i)">
              <PhX :size="12" class="text-danger" />
            </button>
          </div>
          <button type="button" class="text-xs text-text-muted hover:text-text" :disabled="disabled" @click="addInclude">
            <PhPlus :size="12" class="mr-0.5 inline" /> {{ t('workspace.opSettings.addPath') }}
          </button>
        </div>
        <div>
          <div class="mb-1 text-xs font-bold text-text-muted">{{ t('workspace.opSettings.excludePaths') }}</div>
          <div
            v-for="(row, i) in excludeDrafts"
            :key="`exc-${i}`"
            class="mb-1 flex items-center gap-2"
          >
            <input
              v-model="excludeDrafts[i]"
              class="field-input font-mono text-sm"
              placeholder="*.tmp or node_modules/"
              :disabled="disabled"
              @change="emitConfig"
            />
            <button type="button" class="btn-ghost !px-1" :disabled="disabled" @click="removeExclude(i)">
              <PhX :size="12" class="text-danger" />
            </button>
          </div>
          <button type="button" class="text-xs text-text-muted hover:text-text" :disabled="disabled" @click="addExclude">
            <PhPlus :size="12" class="mr-0.5 inline" /> {{ t('workspace.opSettings.addPath') }}
          </button>
        </div>
        <details class="group">
          <summary
            class="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-text-muted hover:text-text"
          >
            <PhCaretDown :size="12" class="transition-transform group-open:rotate-180" />
            {{ t('workspace.opSettings.advanced') }}
          </summary>
          <div class="mt-3 grid grid-cols-2 gap-2">
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.minSize') }}</span>
              <div class="flex gap-1">
                <input
                  v-model="minSizeNum"
                  class="field-input min-w-0 flex-1"
                  type="number"
                  min="0"
                  :disabled="disabled"
                  @change="emitConfig"
                />
                <select v-model="minSizeUnit" class="field-input !w-16 shrink-0" :disabled="disabled" @change="emitConfig">
                  <option v-for="u in sizeUnits" :key="u.value" :value="u.value">{{ u.label }}</option>
                </select>
              </div>
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.maxSize') }}</span>
              <div class="flex gap-1">
                <input
                  v-model="maxSizeNum"
                  class="field-input min-w-0 flex-1"
                  type="number"
                  min="0"
                  :disabled="disabled"
                  @change="emitConfig"
                />
                <select v-model="maxSizeUnit" class="field-input !w-16 shrink-0" :disabled="disabled" @change="emitConfig">
                  <option v-for="u in sizeUnits" :key="u.value" :value="u.value">{{ u.label }}</option>
                </select>
              </div>
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.maxAge') }}</span>
              <div class="flex gap-1">
                <input
                  v-model="maxAgeNum"
                  class="field-input min-w-0 flex-1"
                  type="number"
                  min="0"
                  :disabled="disabled"
                  @change="emitConfig"
                />
                <select v-model="maxAgeUnit" class="field-input !w-16 shrink-0" :disabled="disabled" @change="emitConfig">
                  <option v-for="u in ageUnits" :key="u.value" :value="u.value">{{ u.label }}</option>
                </select>
              </div>
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.minAge') }}</span>
              <div class="flex gap-1">
                <input
                  v-model="minAgeNum"
                  class="field-input min-w-0 flex-1"
                  type="number"
                  min="0"
                  :disabled="disabled"
                  @change="emitConfig"
                />
                <select v-model="minAgeUnit" class="field-input !w-16 shrink-0" :disabled="disabled" @change="emitConfig">
                  <option v-for="u in ageUnits" :key="u.value" :value="u.value">{{ u.label }}</option>
                </select>
              </div>
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.maxDepth') }}</span>
              <input
                class="field-input"
                type="number"
                :value="cfg.maxDepth ?? ''"
                :disabled="disabled"
                @change="patchNum('maxDepth', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.excludeIfPresent') }}</span>
              <input
                class="field-input"
                placeholder=".nosync"
                :value="cfg.excludeIfPresent ?? ''"
                :disabled="disabled"
                @change="patchStr('excludeIfPresent', ($event.target as HTMLInputElement).value)"
              />
            </label>
          </div>
          <div class="mt-2 flex flex-wrap gap-4">
            <AppCheckbox
              :model-value="!!cfg.useRegex"
              :disabled="disabled"
              :label="t('workspace.opSettings.useRegex')"
              @update:model-value="patchBool('useRegex', !!$event)"
            />
            <AppCheckbox
              :model-value="!!cfg.deleteExcluded"
              :disabled="disabled"
              :label="t('workspace.opSettings.deleteExcluded')"
              @update:model-value="patchBool('deleteExcluded', !!$event)"
            />
          </div>
        </details>
      </div>
    </section>

    <!-- Safety -->
    <details class="group border-2 border-border">
      <summary
        class="flex cursor-pointer list-none items-center gap-2 border-l-4 border-l-[var(--color-warning,#b58900)] bg-bg-secondary px-3 py-2 text-xs font-bold uppercase tracking-wide"
      >
        {{ t('workspace.opSettings.safety') }}
        <PhCaretDown :size="12" class="ml-auto transition-transform group-open:rotate-180" />
      </summary>
      <div class="space-y-3 border-t-2 border-border p-3">
        <div class="grid grid-cols-2 gap-2">
          <label class="field-label !mb-0">
            <span>{{ t('workspace.opSettings.maxDelete') }}</span>
            <input
              class="field-input"
              type="number"
              placeholder="100"
              :value="cfg.maxDelete ?? ''"
              :disabled="disabled"
              @change="patchNum('maxDelete', ($event.target as HTMLInputElement).value)"
            />
          </label>
          <label class="field-label !mb-0">
            <span>{{ t('workspace.opSettings.maxTransfer') }}</span>
            <input
              class="field-input"
              placeholder="10G"
              :value="cfg.maxTransfer ?? ''"
              :disabled="disabled"
              @change="patchStr('maxTransfer', ($event.target as HTMLInputElement).value)"
            />
          </label>
          <label class="field-label !mb-0">
            <span>{{ t('workspace.opSettings.maxDeleteSize') }}</span>
            <input
              class="field-input"
              placeholder="1G"
              :value="cfg.maxDeleteSize ?? ''"
              :disabled="disabled"
              @change="patchStr('maxDeleteSize', ($event.target as HTMLInputElement).value)"
            />
          </label>
          <label class="field-label !mb-0">
            <span>{{ t('workspace.opSettings.suffix') }}</span>
            <input
              class="field-input"
              placeholder=".bak"
              :value="cfg.suffix ?? ''"
              :disabled="disabled"
              @change="patchStr('suffix', ($event.target as HTMLInputElement).value)"
            />
          </label>
          <label class="field-label !mb-0 sm:col-span-2">
            <span>{{ t('workspace.opSettings.backupPath') }}</span>
            <input
              class="field-input"
              :value="cfg.backupPath ?? ''"
              :disabled="disabled"
              @change="patchStr('backupPath', ($event.target as HTMLInputElement).value)"
            />
          </label>
        </div>
        <div class="flex flex-wrap gap-4">
          <AppCheckbox
            :model-value="!!cfg.immutable"
            :disabled="disabled"
            :label="t('workspace.opSettings.immutable')"
            @update:model-value="patchBool('immutable', !!$event)"
          />
          <AppCheckbox
            :model-value="!!cfg.suffixKeepExtension"
            :disabled="disabled"
            :label="t('workspace.opSettings.suffixKeepExt')"
            @update:model-value="patchBool('suffixKeepExtension', !!$event)"
          />
        </div>
      </div>
    </details>

    <!-- Comparison -->
    <details class="group border-2 border-border">
      <summary
        class="flex cursor-pointer list-none items-center gap-2 border-l-4 border-l-[var(--color-success,#859900)] bg-bg-secondary px-3 py-2 text-xs font-bold uppercase tracking-wide"
      >
        {{ t('workspace.opSettings.comparison') }}
        <PhCaretDown :size="12" class="ml-auto transition-transform group-open:rotate-180" />
      </summary>
      <div class="flex flex-wrap gap-4 border-t-2 border-border p-3">
        <AppCheckbox
          :model-value="!!cfg.sizeOnly"
          :disabled="disabled"
          :label="t('workspace.opSettings.sizeOnly')"
          @update:model-value="patchBool('sizeOnly', !!$event)"
        />
        <AppCheckbox
          :model-value="!!cfg.updateMode"
          :disabled="disabled"
          :label="t('workspace.opSettings.updateMode')"
          @update:model-value="patchBool('updateMode', !!$event)"
        />
        <AppCheckbox
          :model-value="!!cfg.ignoreExisting"
          :disabled="disabled"
          :label="t('workspace.opSettings.ignoreExisting')"
          @update:model-value="patchBool('ignoreExisting', !!$event)"
        />
      </div>
    </details>

    <!-- Sync options (push) -->
    <section v-if="isPush" class="border-2 border-border">
      <div
        class="flex items-center gap-2 border-b-2 border-border border-l-4 border-l-[var(--color-info,#2aa198)] bg-bg-secondary px-3 py-2"
      >
        <span class="text-xs font-bold uppercase tracking-wide">{{
          t('workspace.opSettings.syncOptions')
        }}</span>
      </div>
      <div class="p-3">
        <label class="field-label !mb-0">
          <span>{{ t('workspace.opSettings.deleteTiming') }}</span>
          <select
            class="field-input"
            :value="cfg.deleteTiming ?? ''"
            :disabled="disabled"
            @change="patchStr('deleteTiming', ($event.target as HTMLSelectElement).value)"
          >
            <option v-for="o in deleteTimingOptions" :key="o.value || 'def'" :value="o.value">
              {{ t(o.labelKey) }}
            </option>
          </select>
        </label>
      </div>
    </section>

    <!-- Bisync options -->
    <section v-if="isBi" class="border-2 border-border">
      <div
        class="flex items-center gap-2 border-b-2 border-border border-l-4 border-l-[var(--color-danger,#d33682)] bg-bg-secondary px-3 py-2"
      >
        <span class="text-xs font-bold uppercase tracking-wide">{{
          t('workspace.opSettings.bisyncOptions')
        }}</span>
      </div>
      <div class="space-y-3 p-3">
        <label class="field-label !mb-0">
          <span>{{ t('workspace.opSettings.conflictResolution') }}</span>
          <select
            class="field-input"
            :value="cfg.conflictResolution ?? ''"
            :disabled="disabled"
            @change="patchStr('conflictResolution', ($event.target as HTMLSelectElement).value)"
          >
            <option value="">{{ t('common.select') }}</option>
            <option v-for="o in conflictOptions" :key="o.value" :value="o.value">
              {{ t(o.labelKey) }}
            </option>
          </select>
        </label>
        <details class="group">
          <summary
            class="flex cursor-pointer list-none items-center gap-1 text-xs font-medium text-text-muted hover:text-text"
          >
            <PhCaretDown :size="12" class="transition-transform group-open:rotate-180" />
            {{ t('workspace.opSettings.advanced') }}
          </summary>
          <div class="mt-3 grid grid-cols-2 gap-2">
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.conflictLoser') }}</span>
              <select
                class="field-input"
                :value="cfg.conflictLoser ?? ''"
                :disabled="disabled"
                @change="patchStr('conflictLoser', ($event.target as HTMLSelectElement).value)"
              >
                <option value="">{{ t('common.select') }}</option>
                <option v-for="o in conflictLoserOptions" :key="o.value" :value="o.value">
                  {{ t(o.labelKey) }}
                </option>
              </select>
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.conflictSuffix') }}</span>
              <input
                class="field-input"
                :value="cfg.conflictSuffix ?? ''"
                :disabled="disabled"
                @change="patchStr('conflictSuffix', ($event.target as HTMLInputElement).value)"
              />
            </label>
            <label class="field-label !mb-0">
              <span>{{ t('workspace.opSettings.maxLock') }}</span>
              <input
                class="field-input"
                placeholder="15m"
                :value="cfg.maxLock ?? ''"
                :disabled="disabled"
                @change="patchStr('maxLock', ($event.target as HTMLInputElement).value)"
              />
            </label>
          </div>
          <div class="mt-2 flex flex-wrap gap-4">
            <AppCheckbox
              :model-value="!!cfg.resilient"
              :disabled="disabled"
              :label="t('workspace.opSettings.resilient')"
              @update:model-value="patchBool('resilient', !!$event)"
            />
            <AppCheckbox
              :model-value="!!cfg.checkAccess"
              :disabled="disabled"
              :label="t('workspace.opSettings.checkAccess')"
              @update:model-value="patchBool('checkAccess', !!$event)"
            />
          </div>
        </details>
      </div>
    </section>
  </div>
</template>
