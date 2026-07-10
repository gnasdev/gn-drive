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

const props = withDefaults(
  defineProps<{
    modelValue: string
    remotes: Remote[]
    /** Base data-testid: local path input uses this; remote mode uses `${testId}-remote` / `${testId}-path`. */
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
    browseError.value = e?.message ?? 'browse failed'
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
  // remote:path
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
    // rclone may return path relative or full
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
  <div class="remote-path-field">
    <div v-if="label" class="field-label">{{ label }}</div>
    <div class="mode-row">
      <button
        type="button"
        class="mode-btn"
        :class="{ active: mode === 'local' }"
        :data-testid="`${testId}-mode-local`"
        @click="setMode('local')"
      >
        Local
      </button>
      <button
        type="button"
        class="mode-btn"
        :class="{ active: mode === 'remote' }"
        :data-testid="`${testId}-mode-remote`"
        @click="setMode('remote')"
      >
        Remote
      </button>
    </div>

    <div class="path-row">
      <select
        v-if="mode === 'remote'"
        v-model="remoteName"
        :data-testid="remoteSelectId"
        class="remote-select"
        @change="emitValue"
      >
        <option value="" disabled>Select remote</option>
        <option v-for="r in remotes" :key="r.name" :value="r.name">
          {{ r.name }}{{ r.type ? ` (${r.type})` : '' }}
        </option>
      </select>
      <input
        v-model="pathPart"
        :data-testid="pathInputId"
        :placeholder="mode === 'local' ? '/absolute/path' : 'folder/path'"
        :required="required"
        class="path-input"
        @change="emitValue"
        @input="emitValue"
      />
      <button
        type="button"
        class="ghost browse-btn"
        :disabled="!canBrowse"
        :data-testid="`${testId}-browse`"
        title="Browse"
        @click="openBrowse"
      >
        <PhMagnifyingGlass :size="14" weight="bold" />
        Browse
      </button>
    </div>

    <div v-if="showBrowse" class="browse-panel" :data-testid="`${testId}-browse-panel`">
      <div class="browse-head">
        <button type="button" class="ghost small" :disabled="browseBusy || parentPath(browseCursor) == null" @click="goParent">
          <PhCaretUp :size="14" weight="bold" /> Up
        </button>
        <code class="cursor mono">{{ browseCursor }}</code>
        <button type="button" class="primary small" :disabled="browseBusy" :data-testid="`${testId}-browse-use`" @click="useBrowsePath">
          Use path
        </button>
        <button type="button" class="ghost small" @click="showBrowse = false">Close</button>
      </div>
      <p v-if="browseError" class="browse-err">{{ browseError }}</p>
      <p v-else-if="browseBusy" class="browse-muted">Loading…</p>
      <div v-else class="browse-list">
        <button
          v-for="e in browseEntries"
          :key="e.path || e.name"
          type="button"
          class="browse-row"
          :class="{ dir: e.is_dir }"
          @click="onEntryClick(e)"
        >
          <PhFolder v-if="e.is_dir" :size="14" weight="regular" />
          <PhFile v-else :size="14" weight="regular" />
          <span class="mono">{{ e.name }}</span>
        </button>
        <p v-if="browseEntries.length === 0" class="browse-muted">Empty directory</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.remote-path-field { display: flex; flex-direction: column; gap: 6px; width: 100%; }
.field-label { font-size: 11px; color: var(--color-text-muted); font-weight: 500; }
.mode-row { display: flex; gap: 4px; }
.mode-btn {
  padding: 4px 10px; font-size: 11px; border-radius: 4px;
  border: 1px solid var(--color-border); background: transparent; color: var(--color-text-muted);
}
.mode-btn.active {
  background: color-mix(in srgb, var(--color-accent) 18%, transparent);
  border-color: var(--color-accent); color: var(--color-accent); font-weight: 600;
}
.path-row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
.remote-select, .path-input {
  padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border);
  border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px;
}
.remote-select { min-width: 140px; max-width: 200px; }
.path-input { flex: 1; min-width: 160px; }
.remote-select:focus, .path-input:focus {
  outline: none; border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent);
}
.ghost {
  display: inline-flex; align-items: center; gap: 4px; padding: 6px 10px;
  background: transparent; border: 1px solid var(--color-border); border-radius: 6px;
  color: var(--color-text); font-size: 12px;
}
.ghost:hover:not(:disabled) { background: var(--color-surface-hover); }
.ghost:disabled { opacity: 0.5; }
.ghost.small { padding: 4px 8px; font-size: 11px; }
.primary {
  display: inline-flex; align-items: center; padding: 4px 10px; background: var(--color-accent);
  color: white; border: 0; border-radius: 6px; font-size: 11px; font-weight: 500;
}
.primary.small { padding: 4px 8px; }
.browse-btn { white-space: nowrap; }

.browse-panel {
  margin-top: 4px; padding: 10px; background: var(--color-bg);
  border: 1px solid var(--color-border); border-radius: 6px;
}
.browse-head { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; margin-bottom: 8px; }
.cursor { font-size: 11px; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mono { font-family: var(--font-mono); }
.browse-err { color: var(--color-danger); font-size: 12px; margin: 0 0 6px; }
.browse-muted { color: var(--color-text-dim); font-size: 12px; margin: 0; }
.browse-list { max-height: 180px; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
.browse-row {
  display: flex; align-items: center; gap: 6px; padding: 5px 8px; text-align: left;
  background: transparent; border: 0; border-radius: 4px; color: var(--color-text); font-size: 12px;
}
.browse-row.dir { cursor: pointer; }
.browse-row.dir:hover { background: var(--color-surface-hover); }
.browse-row:not(.dir) { opacity: 0.7; cursor: default; }
</style>
