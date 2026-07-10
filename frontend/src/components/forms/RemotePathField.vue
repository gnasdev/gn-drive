<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { PhFolder, PhFile, PhCaretUp, PhMagnifyingGlass } from '@phosphor-icons/vue'
import type { Remote, FileEntry } from '@/api/types'
import { api } from '@/api/client'
import {
  browseRoot,
  composeRemotePath,
  parseRemotePath,
} from '@/constants/forms'
import { cn } from '@/lib/cn'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = withDefaults(
  defineProps<{
    modelValue: string
    remotes: Remote[]
    testId?: string
    label?: string
    required?: boolean
  }>(),
  {
    testId: 'path-field',
    label: '',
    required: false,
  },
)

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()

const mode = ref<'local' | 'remote'>('local')
const remoteName = ref('')
const pathPart = ref('')

const showBrowse = ref(false)
const browseBusy = ref(false)
const browseError = ref<string | null>(null)
const browseEntries = ref<FileEntry[]>([])
const browseCursor = ref('')

function applyParsed(value: string) {
  const parsed = parseRemotePath(value)
  mode.value = parsed.mode
  remoteName.value = parsed.remote
  pathPart.value = parsed.path
}

watch(
  () => props.modelValue,
  (v) => {
    const composed = composeRemotePath(mode.value, remoteName.value, pathPart.value)
    if ((v ?? '') !== composed) {
      applyParsed(v ?? '')
    }
  },
  { immediate: true },
)

function emitValue() {
  emit(
    'update:modelValue',
    composeRemotePath(mode.value, remoteName.value, pathPart.value),
  )
}

function setMode(m: 'local' | 'remote') {
  mode.value = m
  if (m === 'remote' && !remoteName.value && props.remotes.length > 0) {
    remoteName.value = props.remotes[0].name
  }
  emitValue()
}

watch([remoteName, pathPart], () => emitValue())

const canBrowse = computed(() => {
  if (mode.value === 'local') return true
  return !!remoteName.value
})

async function openBrowse() {
  if (!canBrowse.value) return
  showBrowse.value = true
  browseError.value = null
  const root = browseRoot(mode.value, remoteName.value, pathPart.value)
  browseCursor.value = root || (mode.value === 'local' ? '/' : `${remoteName.value}:`)
  await loadBrowse(browseCursor.value)
}

async function loadBrowse(remotePath: string) {
  browseBusy.value = true
  browseError.value = null
  try {
    const entries =
      (await api.get<FileEntry[]>(
        `/api/v1/operations/fs?remote=${encodeURIComponent(remotePath)}`,
      )) ?? []
    browseEntries.value = entries
    browseCursor.value = remotePath
  } catch (e: any) {
    browseError.value = e?.message ?? t('pathField.browseFailed')
    browseEntries.value = []
  } finally {
    browseBusy.value = false
  }
}

function parentPath(current: string): string | null {
  if (mode.value === 'local') {
    if (!current || current === '/') return null
    const trimmed = current.replace(/\/+$/, '')
    const idx = trimmed.lastIndexOf('/')
    if (idx <= 0) return '/'
    return trimmed.slice(0, idx) || '/'
  }
  const colon = current.indexOf(':')
  if (colon < 0) return null
  const name = current.slice(0, colon)
  let p = current.slice(colon + 1).replace(/^\/+/, '').replace(/\/+$/, '')
  if (!p) return null
  const idx = p.lastIndexOf('/')
  if (idx < 0) return `${name}:`
  return `${name}:/${p.slice(0, idx)}`
}

async function goParent() {
  const p = parentPath(browseCursor.value)
  if (p == null) return
  await loadBrowse(p)
}

function entryFullPath(e: FileEntry): string {
  if (e.path) {
    if (mode.value === 'local') {
      return e.path.startsWith('/') ? e.path : `${browseCursor.value.replace(/\/+$/, '')}/${e.name}`
    }
    if (e.path.includes(':')) return e.path
    const base = browseCursor.value
    if (base.endsWith(':')) return `${base}/${e.name}`
    return `${base.replace(/\/+$/, '')}/${e.name}`
  }
  if (mode.value === 'local') {
    const base = browseCursor.value.replace(/\/+$/, '') || ''
    return `${base}/${e.name}`.replace(/\/+/g, '/')
  }
  const base = browseCursor.value
  if (base.endsWith(':')) return `${base}/${e.name}`
  return `${base.replace(/\/+$/, '')}/${e.name}`
}

async function onEntryClick(e: FileEntry) {
  if (!e.is_dir) return
  await loadBrowse(entryFullPath(e))
}

function useBrowsePath() {
  const cur = browseCursor.value
  if (mode.value === 'local') {
    pathPart.value = cur || '/'
  } else {
    const colon = cur.indexOf(':')
    if (colon >= 0) {
      remoteName.value = cur.slice(0, colon)
      pathPart.value = cur.slice(colon + 1).replace(/^\/+/, '')
    }
  }
  emitValue()
  showBrowse.value = false
}

const remoteSelectId = computed(() => `${props.testId}-remote`)
const pathInputId = computed(() =>
  mode.value === 'local' ? props.testId : `${props.testId}-path`,
)
</script>

<template>
  <div class="flex w-full flex-col gap-1.5">
    <div v-if="label" class="text-[11px] font-medium text-text-muted">{{ label }}</div>
    <div class="flex gap-1">
      <button
        type="button"
        :class="cn(
          'rounded border border-border bg-transparent px-2.5 py-1 text-[11px] text-text-muted',
          mode === 'local' && 'border-accent bg-accent/15 font-semibold text-accent',
        )"
        :data-testid="`${testId}-mode-local`"
        @click="setMode('local')"
      >
        {{ t('common.local') }}
      </button>
      <button
        type="button"
        :class="cn(
          'rounded border border-border bg-transparent px-2.5 py-1 text-[11px] text-text-muted',
          mode === 'remote' && 'border-accent bg-accent/15 font-semibold text-accent',
        )"
        :data-testid="`${testId}-mode-remote`"
        @click="setMode('remote')"
      >
        {{ t('common.remote') }}
      </button>
    </div>

    <div class="flex flex-wrap items-center gap-1.5">
      <select
        v-if="mode === 'remote'"
        v-model="remoteName"
        :data-testid="remoteSelectId"
        class="field-input max-w-[200px] min-w-[140px]"
        @change="emitValue"
      >
        <option value="" disabled>{{ t('common.selectRemote') }}</option>
        <option v-for="r in remotes" :key="r.name" :value="r.name">
          {{ r.name }}{{ r.type ? ` (${r.type})` : '' }}
        </option>
      </select>
      <input
        v-model="pathPart"
        :data-testid="pathInputId"
        :placeholder="mode === 'local' ? t('pathField.absolutePlaceholder') : t('pathField.folderPlaceholder')"
        :required="required"
        class="field-input min-w-[160px] flex-1"
        @change="emitValue"
        @input="emitValue"
      />
      <button
        type="button"
        class="btn-ghost whitespace-nowrap"
        :disabled="!canBrowse"
        :data-testid="`${testId}-browse`"
        :title="t('common.browse')"
        @click="openBrowse"
      >
        <PhMagnifyingGlass :size="14" weight="bold" />
        {{ t('common.browse') }}
      </button>
    </div>

    <div
      v-if="showBrowse"
      class="mt-1 rounded-md border border-border bg-bg p-2.5"
      :data-testid="`${testId}-browse-panel`"
    >
      <div class="mb-2 flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="btn-ghost !px-2 !py-1 !text-[11px]"
          :disabled="browseBusy || parentPath(browseCursor) == null"
          @click="goParent"
        >
          <PhCaretUp :size="14" weight="bold" /> {{ t('common.up') }}
        </button>
        <code class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[11px]">
          {{ browseCursor }}
        </code>
        <button
          type="button"
          class="btn-primary !px-2 !py-1 !text-[11px]"
          :disabled="browseBusy"
          :data-testid="`${testId}-browse-use`"
          @click="useBrowsePath"
        >
          {{ t('common.usePath') }}
        </button>
        <button type="button" class="btn-ghost !px-2 !py-1 !text-[11px]" @click="showBrowse = false">
          {{ t('common.close') }}
        </button>
      </div>
      <p v-if="browseError" class="m-0 mb-1.5 text-xs text-danger">{{ browseError }}</p>
      <p v-else-if="browseBusy" class="m-0 text-xs text-text-dim">{{ t('common.loadingDots') }}</p>
      <div v-else class="flex max-h-[180px] flex-col gap-0.5 overflow-auto">
        <button
          v-for="e in browseEntries"
          :key="e.path || e.name"
          type="button"
          :class="cn(
            'flex items-center gap-1.5 rounded px-2 py-1 text-left text-xs text-text',
            e.is_dir ? 'cursor-pointer hover:bg-surface-hover' : 'cursor-default opacity-70',
          )"
          @click="onEntryClick(e)"
        >
          <PhFolder v-if="e.is_dir" :size="14" weight="regular" />
          <PhFile v-else :size="14" weight="regular" />
          <span class="font-mono">{{ e.name }}</span>
        </button>
        <p v-if="browseEntries.length === 0" class="m-0 text-xs text-text-dim">{{ t('common.emptyDir') }}</p>
      </div>
    </div>
  </div>
</template>
