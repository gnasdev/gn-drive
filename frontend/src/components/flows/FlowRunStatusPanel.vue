<script setup lang="ts">
/**
 * Wails `app-operation-logs-panel`:
 * progress header + stats + file tabs (Syncing / Complete / Failed / Pending).
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  PhSpinner,
  PhCheckCircle,
  PhXCircle,
  PhStop,
  PhCaretDown,
  PhCaretUp,
  PhLightning,
  PhDatabase,
  PhTimer,
  PhMagnifyingGlass,
  PhGear,
  PhFile,
  PhWarningCircle,
} from '@phosphor-icons/vue'
import type { FileTransferInfo, Flow, FlowOpSyncStatus } from '@/api/types'
import { humanizeError, isUserCancelError } from '@/lib/humanizeError'
import { resolveOpAction } from '@/stores/flows'

type FileTabId = 'syncing' | 'complete' | 'failed' | 'pending'

const props = defineProps<{
  flow: Flow
  flowStatus: string
  syncStatus: FlowOpSyncStatus | null
  lastError?: string
}>()

const { t } = useI18n()
const expanded = ref(true)
const fileTab = ref<FileTabId>('syncing')
/** Once the user clicks a tab, never auto-switch until flow status changes. */
const userPickedTab = ref(false)
let lastFlowStatus = props.flowStatus

const ss = computed(() => props.syncStatus)

const progress = computed(() => {
  const p = ss.value?.progress ?? 0
  if (props.flowStatus === 'completed' && p <= 0) return 100
  return Math.max(0, Math.min(100, p))
})

const detailedLabel = computed(() => {
  const st = props.flowStatus
  if (st === 'completed') return t('workspace.syncLabels.completed')
  if (st === 'failed') return t('workspace.syncLabels.error')
  if (st === 'cancelled' || st === 'cancelling') return t('workspace.syncLabels.stopped')
  if (st !== 'running') return t('workspace.syncLabels.waiting')
  const s = ss.value
  if (!s) return t('workspace.syncLabels.preparing')
  if ((s.transfers ?? []).some((f) => f.status === 'transferring')) {
    return t('workspace.syncLabels.transferring')
  }
  if (s.bytes_transferred > 0 || (s.files_transferred > 0 && s.total_files > 0)) {
    return t('workspace.syncLabels.transferring')
  }
  if (s.checks > 0 || s.total_checks > 0) return t('workspace.syncLabels.checking')
  return t('workspace.syncLabels.preparing')
})

const actionLabel = computed(() => {
  const a = (ss.value?.action || activeOpAction.value || 'push').toLowerCase()
  switch (a) {
    case 'bi':
      return t('workspace.actionOptions.bi')
    case 'bi-resync':
      return t('workspace.actionOptions.bi-resync')
    default:
      return t('workspace.actionOptions.push')
  }
})

const activeOpAction = computed(() => {
  const opId = ss.value?.op_id
  const ops = props.flow.operations ?? []
  if (opId) {
    const op = ops.find((o) => o.id === opId)
    if (op) return resolveOpAction(op)
  }
  const running = ops.find((o) => o.status === 'running')
  return running ? resolveOpAction(running) : ops[0] ? resolveOpAction(ops[0]) : 'push'
})

const backendDetail = computed(() => {
  const st = props.flowStatus
  if (st === 'cancelled' || st === 'cancelling') return t('workspace.syncLabels.stoppedDetail')
  if (st === 'completed') {
    const s = ss.value
    if (s && s.total_files > 0) {
      return t('workspace.syncLabels.transferredFiles', {
        n: s.files_transferred,
        total: s.total_files,
      })
    }
    return t('workspace.syncLabels.completedDetail')
  }
  if (st === 'failed') {
    return friendlyError.value || t('workspace.syncLabels.failedDetail')
  }
  const s = ss.value
  if (!s) return t('workspace.syncLabels.waitingBackend')
  const syncing = fileGroups.value.syncing
  if (syncing.length === 1 && !syncing[0].name.startsWith('(')) {
    return syncing[0].name
  }
  if (syncing.length > 1) {
    return t('workspace.syncLabels.syncingN', { n: syncing.length })
  }
  if (s.total_files > 0) {
    return t('workspace.syncLabels.transferredFiles', {
      n: s.files_transferred,
      total: s.total_files,
    })
  }
  if (s.bytes_transferred > 0) {
    return t('workspace.syncLabels.transferredBytes', { bytes: formatBytes(s.bytes_transferred) })
  }
  if (s.total_checks > 0) {
    return t('workspace.syncLabels.checkingOf', { n: s.checks, total: s.total_checks })
  }
  return t('workspace.syncLabels.preparingDetail')
})

const friendlyError = computed(() => {
  if (isUserCancelError(props.lastError, props.flowStatus)) return ''
  if (isUserCancelError(ss.value?.error_message, ss.value?.status)) return ''
  return humanizeError(props.lastError || ss.value?.error_message, props.flowStatus)
})

const barClass = computed(() => {
  switch (props.flowStatus) {
    case 'completed':
      return 'bg-success'
    case 'failed':
      return 'bg-danger'
    case 'cancelled':
    case 'cancelling':
      return 'bg-text-dim'
    default:
      return 'bg-accent-strong'
  }
})

const fileGroups = computed(() => {
  const groups: Record<FileTabId, FileTransferInfo[]> = {
    syncing: [],
    complete: [],
    failed: [],
    pending: [],
  }
  for (const f of ss.value?.transfers ?? []) {
    switch (f.status) {
      case 'transferring':
      case 'checking':
        groups.syncing.push(f)
        break
      case 'completed':
      case 'checked':
        groups.complete.push(f)
        break
      case 'failed':
        groups.failed.push(f)
        break
      case 'pending':
        groups.pending.push(f)
        break
      default:
        groups.complete.push(f)
    }
  }
  return groups
})

const tabCounts = computed(() => ({
  syncing: fileGroups.value.syncing.length,
  complete: fileGroups.value.complete.filter((f) => !f.name.startsWith('(')).length,
  failed: fileGroups.value.failed.length,
  pending: fileGroups.value.pending.reduce((n, f) => {
    // "(12 pending)" synthetic row → parse count
    const m = /^\((\d+) pending\)$/.exec(f.name)
    return n + (m ? Number(m[1]) : 1)
  }, 0),
}))

const selectedFiles = computed(() => fileGroups.value[fileTab.value] ?? [])

const hasFileList = computed(() => {
  const list = ss.value?.transfers ?? []
  return list.length > 0
})

/** Prefer a tab that actually has rows so Pending is not left empty while Syncing is. */
function autoPickTab(st: string) {
  if (userPickedTab.value) return
  const c = tabCounts.value
  if (st === 'failed' && c.failed > 0) {
    fileTab.value = 'failed'
    return
  }
  if (st === 'completed' || st === 'cancelled') {
    fileTab.value = c.complete > 0 ? 'complete' : c.failed > 0 ? 'failed' : c.pending > 0 ? 'pending' : 'complete'
    return
  }
  // running / cancelling / idle-with-snapshot
  if (c.syncing > 0) fileTab.value = 'syncing'
  else if (c.pending > 0) fileTab.value = 'pending'
  else if (c.failed > 0) fileTab.value = 'failed'
  else if (c.complete > 0) fileTab.value = 'complete'
  else fileTab.value = 'syncing'
}

/**
 * React to flowStatus transitions and to transfer population.
 * Once the user clicks a tab, never auto-switch until a new run starts.
 * Do not thrash tabs on every progress tick: only move when current tab is empty
 * or the flow lifecycle changes.
 */
watch(
  () => props.flowStatus,
  (st, prev) => {
    if (st === lastFlowStatus && st === prev) return
    const statusChanged = st !== lastFlowStatus
    lastFlowStatus = st
    if (!statusChanged && prev !== undefined) return

    // New run: allow auto-pick again.
    if (st === 'running' || st === 'cancelling') {
      if (prev && prev !== 'running' && prev !== 'cancelling') {
        userPickedTab.value = false
      }
      autoPickTab(st)
      return
    }
    autoPickTab(st)
  },
)

watch(
  () => tabCounts.value,
  () => {
    if (userPickedTab.value) return
    const st = props.flowStatus
    const cur = tabCounts.value[fileTab.value] ?? 0
    // Only jump away from an empty tab when another tab has rows (e.g. pending seed landed).
    if (cur > 0) return
    const any =
      tabCounts.value.syncing +
      tabCounts.value.pending +
      tabCounts.value.complete +
      tabCounts.value.failed
    if (any === 0) return
    autoPickTab(st)
  },
)

function selectFileTab(id: FileTabId) {
  userPickedTab.value = true
  fileTab.value = id
}

const fileTabs: { id: FileTabId; labelKey: string }[] = [
  { id: 'syncing', labelKey: 'workspace.fileTabs.syncing' },
  { id: 'complete', labelKey: 'workspace.fileTabs.complete' },
  { id: 'failed', labelKey: 'workspace.fileTabs.failed' },
  { id: 'pending', labelKey: 'workspace.fileTabs.pending' },
]

function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(sizes.length - 1, Math.floor(Math.log(bytes) / Math.log(k)))
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`
}

function formatSpeed(bps: number): string {
  if (!bps || bps <= 0) return '0 B/s'
  return `${formatBytes(bps)}/s`
}

function formatEta(secs: number): string {
  if (!secs || secs <= 0) return t('workspace.syncLabels.etaCalc')
  if (secs < 60) return `${Math.round(secs)}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${Math.round(secs % 60)}s`
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return `${h}h ${m}m`
}

function fileBaseName(path: string): string {
  const parts = path.split(/[/\\]/)
  return parts[parts.length - 1] || path
}

function tabBtnClass(id: FileTabId): string {
  const on = fileTab.value === id
  const base = 'min-w-0 px-2 py-1.5 text-xs font-bold flex items-center justify-center gap-1 border-2 transition-colors'
  if (!on) return `${base} border-transparent text-text-muted hover:bg-bg/50`
  switch (id) {
    case 'syncing':
      return `${base} border-border bg-warning/20 text-running`
    case 'complete':
      return `${base} border-border bg-success/20 text-success`
    case 'failed':
      return `${base} border-border bg-danger/20 text-danger`
    case 'pending':
      return `${base} border-border bg-bg text-text-muted`
  }
}
</script>

<template>
  <div
    class="space-y-3 border-t-2 border-border bg-[var(--color-bg-secondary)] p-4 text-text"
    :data-testid="`flow-run-status-${flow.id}`"
  >
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <div class="flex flex-wrap items-center gap-2">
          <PhSpinner
            v-if="flowStatus === 'running' || flowStatus === 'cancelling'"
            :size="16"
            class="animate-spin text-running shrink-0"
          />
          <PhMagnifyingGlass
            v-else-if="detailedLabel === t('workspace.syncLabels.checking')"
            :size="16"
            class="text-info shrink-0"
            weight="bold"
          />
          <PhGear
            v-else-if="detailedLabel === t('workspace.syncLabels.preparing')"
            :size="16"
            class="text-info shrink-0"
            weight="bold"
          />
          <PhCheckCircle
            v-else-if="flowStatus === 'completed'"
            :size="16"
            class="text-success shrink-0"
            weight="fill"
          />
          <PhXCircle
            v-else-if="flowStatus === 'failed'"
            :size="16"
            class="text-danger shrink-0"
            weight="fill"
          />
          <PhStop v-else :size="16" class="text-text-dim shrink-0" weight="fill" />
          <span class="text-base font-bold">{{ detailedLabel }}</span>
          <span class="text-xs font-bold uppercase text-text-muted">{{ actionLabel }}</span>
        </div>
        <p class="m-0 mt-1 truncate text-xs text-text-muted" :title="backendDetail">
          {{ backendDetail }}
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-3">
        <span class="text-lg font-bold tabular-nums">{{ progress.toFixed(1) }}%</span>
        <button
          type="button"
          class="inline-flex size-7 items-center justify-center border-2 border-border bg-bg hover:bg-surface-hover"
          :aria-expanded="expanded"
          :aria-label="t('workspace.syncLabels.toggleDetails')"
          @click="expanded = !expanded"
        >
          <PhCaretUp v-if="expanded" :size="12" weight="bold" />
          <PhCaretDown v-else :size="12" weight="bold" />
        </button>
      </div>
    </div>

    <div v-if="!expanded" class="h-2 w-full bg-bg">
      <div class="h-full transition-all duration-300" :class="barClass" :style="{ width: `${progress}%` }" />
    </div>

    <template v-else>
      <div class="h-3 w-full border-2 border-border bg-bg">
        <div class="h-full transition-all duration-300" :class="barClass" :style="{ width: `${progress}%` }" />
      </div>

      <div class="grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
        <div class="border-2 border-border bg-bg px-2 py-1">
          <div class="font-bold uppercase text-text-muted">{{ t('workspace.syncLabels.files') }}</div>
          <div class="font-bold tabular-nums">
            {{ ss?.files_transferred ?? 0 }}
            <template v-if="(ss?.total_files ?? 0) > 0"> / {{ ss?.total_files }}</template>
          </div>
        </div>
        <div class="border-2 border-border bg-bg px-2 py-1">
          <div class="font-bold uppercase text-text-muted">{{ t('workspace.syncLabels.checks') }}</div>
          <div class="font-bold tabular-nums">
            {{ ss?.checks ?? 0 }}
            <template v-if="(ss?.total_checks ?? 0) > 0"> / {{ ss?.total_checks }}</template>
          </div>
        </div>
        <div class="border-2 border-border bg-bg px-2 py-1">
          <div class="font-bold uppercase text-text-muted">{{ t('workspace.syncLabels.deletes') }}</div>
          <div class="font-bold tabular-nums">{{ ss?.deletes ?? 0 }}</div>
        </div>
        <div class="border-2 border-border bg-bg px-2 py-1">
          <div class="font-bold uppercase text-text-muted">{{ t('workspace.syncLabels.renames') }}</div>
          <div class="font-bold tabular-nums">{{ ss?.renames ?? 0 }}</div>
        </div>
      </div>

      <div class="flex flex-wrap items-center justify-between gap-3 text-xs font-medium">
        <span class="inline-flex items-center gap-1.5">
          <PhLightning :size="12" weight="bold" />
          {{ formatSpeed(ss?.speed_bps ?? 0) }}
        </span>
        <span class="inline-flex items-center gap-1.5">
          <PhDatabase :size="12" weight="bold" />
          {{ formatBytes(ss?.bytes_transferred ?? 0) }}
          <template v-if="(ss?.total_bytes ?? 0) > 0">
            / {{ formatBytes(ss?.total_bytes ?? 0) }}
          </template>
        </span>
        <span class="inline-flex items-center gap-1.5">
          <PhTimer :size="12" weight="bold" />
          ETA {{ formatEta(ss?.eta_secs ?? 0) }}
        </span>
      </div>

      <!-- Wails file tabs: Syncing / Complete / Failed / Pending -->
      <div v-if="hasFileList" class="border-t-2 border-border pt-3">
        <div
          class="mb-2 grid grid-cols-4 gap-1 bg-bg/40 p-1"
          role="tablist"
          :aria-label="t('workspace.fileTabs.aria')"
        >
          <button
            v-for="tab in fileTabs"
            :key="tab.id"
            type="button"
            role="tab"
            :class="tabBtnClass(tab.id)"
            :aria-selected="fileTab === tab.id"
            @click="selectFileTab(tab.id)"
          >
            <span class="truncate">{{ t(tab.labelKey) }}</span>
            <span class="min-w-5 bg-bg-secondary/80 px-1 py-0.5 text-center text-[10px] leading-none">
              {{ tabCounts[tab.id] }}
            </span>
          </button>
        </div>

        <div class="max-h-48 space-y-1 overflow-auto" role="tabpanel">
          <div
            v-for="file in selectedFiles"
            :key="file.name"
            class="flex items-center gap-2 px-2 py-1.5 text-sm"
            :class="{
              'bg-warning/10': file.status === 'transferring' || file.status === 'checking',
              'bg-success/10': file.status === 'completed' || file.status === 'checked',
              'bg-danger/10': file.status === 'failed',
              'bg-bg/50': file.status === 'pending',
            }"
          >
            <PhSpinner
              v-if="file.status === 'transferring' || file.status === 'checking'"
              :size="14"
              class="shrink-0 animate-spin text-running"
            />
            <PhCheckCircle
              v-else-if="file.status === 'completed' || file.status === 'checked'"
              :size="14"
              class="shrink-0 text-success"
              weight="fill"
            />
            <PhWarningCircle
              v-else-if="file.status === 'failed'"
              :size="14"
              class="shrink-0 text-danger"
              weight="fill"
            />
            <PhFile v-else :size="14" class="shrink-0 text-text-dim" />

            <span class="min-w-0 flex-1 truncate font-medium" :title="file.name">
              {{ fileBaseName(file.name) }}
            </span>

            <template v-if="file.status === 'transferring'">
              <span class="shrink-0 font-bold text-running">{{ file.progress.toFixed(0) }}%</span>
              <span v-if="file.speed" class="shrink-0 text-xs text-text-muted">
                {{ formatSpeed(file.speed) }}
              </span>
            </template>
            <template v-else-if="file.status === 'failed'">
              <span class="max-w-[10rem] shrink-0 truncate text-xs text-danger" :title="file.error">
                {{ file.error || t('workspace.syncLabels.error') }}
              </span>
            </template>
            <template v-else-if="file.status !== 'pending'">
              <span class="shrink-0 text-xs text-text-muted">
                {{ formatBytes(file.bytes || file.size) }}
              </span>
            </template>
          </div>

          <div v-if="!selectedFiles.length" class="px-2 py-3 text-sm text-text-muted">
            <template v-if="fileTab === 'pending'">
              {{ t('workspace.fileTabs.emptyPending') }}
            </template>
            <template v-else>
              {{ t('workspace.fileTabs.empty') }}
            </template>
          </div>
        </div>
      </div>
    </template>

    <p
      v-if="friendlyError"
      class="m-0 border-2 border-danger bg-bg px-2 py-1 text-xs text-danger"
    >
      {{ friendlyError }}
    </p>
  </div>
</template>
