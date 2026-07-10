<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhPlay, PhFolder, PhFile, PhInfo, PhWarning } from '@phosphor-icons/vue'
import { useOperationsStore, type FileOp } from '@/stores/operations'
import { useRemotesStore } from '@/stores/remotes'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppAlert from '@/components/ui/Alert.vue'
import { useToast } from '@/composables/useToast'
import {
  FILE_OP_META,
  FILE_OPS,
  SYNC_ACTION_META,
  SYNC_ACTIONS,
  type SyncAction,
} from '@/constants/forms'
import { cn } from '@/lib/cn'

const { t } = useI18n()
const store = useOperationsStore()
const remotes = useRemotesStore()
const toast = useToast()

const selectedRemote = ref('')
const remoteInput = ref('/')
const lastTaskId = ref<string | null>(null)
const opSource = ref('')
const opDest = ref('')
const opPath = ref('')
const selectedOp = ref<FileOp>('copy')
const syncProfile = ref('')
const focusedSync = ref<SyncAction>('push')

const selectedProfile = computed(() =>
  store.profiles.find((p) => p.name === syncProfile.value) ?? null,
)

const syncRisk = computed(() => SYNC_ACTION_META[focusedSync.value].risk)
const fileOpMeta = computed(() => FILE_OP_META[selectedOp.value])
const needsSourceDest = computed(() => fileOpMeta.value.fields === 'source-dest')

onMounted(async () => {
  await Promise.all([store.loadProfiles(), store.loadTasks(), remotes.load()])
  if (store.profiles.length > 0) {
    syncProfile.value = store.profiles[0].name
  }
})

watch(selectedRemote, (name) => {
  if (name) {
    remoteInput.value = `${name}:`
  }
})

async function doBrowse() {
  if (!remoteInput.value) return
  try {
    await store.browse(remoteInput.value)
  } catch {
    // store.error already set
  }
}

async function doStartSync(action: SyncAction) {
  focusedSync.value = action
  const profileName = syncProfile.value || store.profiles[0]?.name
  if (!profileName) {
    toast.error(t('operations.createProfileFirst'))
    return
  }
  lastTaskId.value = await store.startSync(action, profileName)
  if (lastTaskId.value) {
    setTimeout(() => store.loadTasks(), 500)
  }
}

async function doFileOp() {
  try {
    if (selectedOp.value === 'mkdir' || selectedOp.value === 'purge' || selectedOp.value === 'delete') {
      await store.runOp(selectedOp.value, { path: opPath.value || opDest.value || opSource.value })
    } else {
      await store.runOp(selectedOp.value, { source: opSource.value, dest: opDest.value })
    }
    toast.success(store.lastOpResult ?? 'ok')
    if (remoteInput.value) await store.browse(remoteInput.value)
  } catch (e: any) {
    toast.error(e?.message ?? 'operation failed')
  }
}

function riskClass(risk?: string) {
  if (risk === 'danger') return 'border-danger/25 bg-danger/8 text-danger'
  if (risk === 'caution') return 'border-warning/25 bg-warning/8 text-warning'
  return 'border-border bg-bg text-text-muted'
}
</script>

<template>
  <div class="page-shell" data-testid="page-operations">
    <header class="mb-5">
      <h1 class="page-title">{{ t('operations.title') }}</h1>
      <p class="page-sub">{{ t('operations.sub') }}</p>
    </header>

    <section class="card mb-3 px-5 py-4">
      <h3 class="section-label">{{ t('operations.quickSync') }}</h3>
      <p class="mb-2 text-xs text-text-dim">{{ t('operations.quickSyncHelp') }}</p>

      <label class="field-label mb-2.5 max-w-sm">
        <span>{{ t('operations.profile') }}</span>
        <select v-model="syncProfile" class="field-input" data-testid="ops-sync-profile">
          <option value="" disabled>{{ t('common.selectProfile') }}</option>
          <option v-for="p in store.profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
      </label>

      <div
        v-if="selectedProfile"
        class="mb-3 rounded-md border border-border bg-bg px-3 py-2 font-mono text-[11px] text-text-muted"
        data-testid="ops-sync-profile-paths"
      >
        <span class="text-text-dim">{{ t('operations.from') }}</span> {{ selectedProfile.from || t('common.empty') }}
        <span class="mx-1.5 text-text-dim">→</span>
        <span class="text-text-dim">{{ t('operations.to') }}</span> {{ selectedProfile.to || t('common.empty') }}
        <span v-if="selectedProfile.direction" class="ml-2 badge">
          {{ t('operations.defaultDirection', { dir: selectedProfile.direction }) }}
        </span>
      </div>

      <div class="mb-3 flex flex-wrap gap-1.5">
        <button
          v-for="a in SYNC_ACTIONS"
          :key="a"
          type="button"
          :class="cn(
            'btn-ghost font-mono',
            focusedSync === a && 'border-accent bg-accent/10 text-accent',
          )"
          :title="t(`syncHelp.${a}.summary`)"
          :data-testid="`ops-sync-${a}`"
          @mouseenter="focusedSync = a"
          @focus="focusedSync = a"
          @click="doStartSync(a)"
        >
          <PhPlay :size="14" weight="bold" /> {{ a }}
        </button>
      </div>

      <div
        class="rounded-md border px-3 py-2.5"
        :class="riskClass(syncRisk)"
        data-testid="ops-sync-help"
      >
        <div class="mb-1 flex items-center gap-1.5 text-[12px] font-semibold text-text">
          <PhWarning v-if="syncRisk === 'danger' || syncRisk === 'caution'" :size="14" weight="bold" />
          <PhInfo v-else :size="14" weight="bold" />
          <span class="font-mono">{{ focusedSync }}</span>
          <span class="font-sans font-normal text-text-muted">· {{ t(`syncHelp.${focusedSync}.summary`) }}</span>
        </div>
        <p class="m-0 text-[12px] leading-relaxed text-text-muted">{{ t(`syncHelp.${focusedSync}.detail`) }}</p>
      </div>

      <details class="mt-3">
        <summary class="cursor-pointer text-[11px] font-medium text-text-dim hover:text-text-muted">
          {{ t('operations.allSyncGlance') }}
        </summary>
        <dl class="mt-2 grid gap-2 sm:grid-cols-2">
          <div
            v-for="a in SYNC_ACTIONS"
            :key="a"
            class="rounded-md border border-border bg-bg px-2.5 py-2"
          >
            <dt class="font-mono text-[12px] font-semibold text-text">{{ a }}</dt>
            <dd class="m-0 mt-0.5 text-[11px] leading-snug text-text-muted">
              {{ t(`syncHelp.${a}.summary`) }}. {{ t(`syncHelp.${a}.detail`) }}
            </dd>
          </div>
        </dl>
      </details>

      <AppAlert v-if="lastTaskId" type="success" class="mt-3" data-testid="ops-task-started">
        {{ t('operations.startedTask') }} <code class="font-mono">{{ lastTaskId }}</code>
      </AppAlert>
    </section>

    <section class="card mb-3 px-5 py-4">
      <h3 class="section-label">{{ t('operations.activeTasks') }}</h3>
      <p class="mb-2 text-xs text-text-dim">{{ t('operations.activeTasksHelp') }}</p>
      <div v-if="store.tasks.length === 0"><EmptyState :title="t('operations.noTasks')" /></div>
      <div v-else class="flex flex-col gap-1.5" data-testid="ops-task-list">
        <div
          v-for="task in store.tasks"
          :key="task.id"
          class="flex items-center justify-between rounded-md bg-bg px-2.5 py-2"
        >
          <div class="text-[13px]">
            {{ task.name }} <span class="badge ml-1">{{ task.action }}</span>
          </div>
          <div class="flex items-center gap-2">
            <span
              :class="cn(
                'rounded px-1.5 py-px text-[11px]',
                task.status === 'running' && 'bg-running/15 text-running',
                task.status === 'completed' && 'bg-completed/15 text-completed',
                task.status === 'failed' && 'bg-failed/15 text-failed',
                task.status === 'cancelled' && 'bg-warning/15 text-warning',
              )"
            >
              {{ task.status }}
            </span>
            <span v-if="task.transferred" class="font-mono text-[11px]">{{ task.transferred }} B</span>
          </div>
        </div>
      </div>
    </section>

    <section class="card mb-3 px-5 py-4">
      <h3 class="section-label">{{ t('operations.browse') }}</h3>
      <p class="mb-2 text-xs text-text-dim">
        <i18n-t keypath="operations.browseHelp" tag="span">
          <template #local>
            <code class="rounded bg-bg px-1 font-mono">/tmp</code>
          </template>
          <template #remote>
            <code class="rounded bg-bg px-1 font-mono">remote:path</code>
          </template>
        </i18n-t>
      </p>
      <div class="mb-2 flex flex-wrap gap-1.5">
        <select v-model="selectedRemote" class="field-input min-w-[140px]" data-testid="ops-browse-remote">
          <option value="">{{ t('operations.customAbsolute') }}</option>
          <option v-for="r in remotes.items" :key="r.name" :value="r.name">
            {{ r.name }}{{ r.type ? ` (${r.type})` : '' }}
          </option>
        </select>
        <input
          v-model="remoteInput"
          placeholder="remote:/path or /absolute"
          class="field-input min-w-[160px] flex-1"
          data-testid="ops-browse-path"
          @keydown.enter="doBrowse"
        />
        <button class="btn-primary" data-testid="ops-browse-submit" :disabled="store.busy" @click="doBrowse">
          {{ store.busy ? t('common.loading') : t('common.browse') }}
        </button>
      </div>
      <AppAlert v-if="store.error" type="error" data-testid="ops-browse-error">{{ store.error }}</AppAlert>
      <div v-if="store.entries.length > 0" class="overflow-hidden rounded-md bg-bg" data-testid="ops-browse-list">
        <div class="grid grid-cols-[1fr_100px_180px] gap-3 px-3 py-1.5 text-[10px] uppercase tracking-wide text-text-dim">
          <span>{{ t('operations.colName') }}</span>
          <span>{{ t('operations.colSize') }}</span>
          <span>{{ t('operations.colModified') }}</span>
        </div>
        <div
          v-for="e in store.entries"
          :key="e.path || e.name"
          class="grid grid-cols-[1fr_100px_180px] items-center gap-3 border-t border-border px-3 py-1.5 text-xs"
          data-testid="ops-browse-row"
        >
          <span class="flex items-center gap-1.5">
            <PhFolder v-if="e.is_dir" :size="14" weight="regular" />
            <PhFile v-else :size="14" weight="regular" />
            <span class="font-mono text-[11px]">{{ e.name }}</span>
          </span>
          <span class="font-mono text-[11px]">{{ e.is_dir ? t('common.empty') : e.size + ' B' }}</span>
          <span class="text-[11px] text-text-muted">{{ e.mod_time }}</span>
        </div>
      </div>
    </section>

    <section class="card mb-3 px-5 py-4">
      <h3 class="section-label">{{ t('operations.fileOps') }}</h3>
      <p class="mb-2 text-xs text-text-dim">
        <i18n-t keypath="operations.fileOpsHelp" tag="span">
          <template #local>
            <code class="rounded bg-bg px-1 font-mono">/absolute</code>
          </template>
          <template #remote>
            <code class="rounded bg-bg px-1 font-mono">remote:path</code>
          </template>
        </i18n-t>
      </p>

      <div class="grid grid-cols-1 items-end gap-2.5 md:grid-cols-[140px_1fr_1fr_auto]">
        <label class="field-label">
          <span>{{ t('operations.op') }}</span>
          <select v-model="selectedOp" class="field-input" data-testid="ops-file-op">
            <option v-for="op in FILE_OPS" :key="op" :value="op">{{ op }}</option>
          </select>
        </label>
        <label v-if="needsSourceDest" class="field-label">
          <span>{{ t('operations.source') }}</span>
          <input
            v-model="opSource"
            placeholder="/tmp/src or remote:path"
            class="field-input"
            data-testid="ops-file-source"
          />
          <span class="text-[10px] font-normal text-text-dim">{{ t('operations.sourceHint') }}</span>
        </label>
        <label v-if="needsSourceDest" class="field-label">
          <span>{{ t('operations.dest') }}</span>
          <input
            v-model="opDest"
            placeholder="/tmp/dst or remote:path"
            class="field-input"
            data-testid="ops-file-dest"
          />
          <span class="text-[10px] font-normal text-text-dim">{{ t('operations.destHint') }}</span>
        </label>
        <label v-if="!needsSourceDest" class="field-label md:col-span-2">
          <span>{{ t('common.path') }}</span>
          <input
            v-model="opPath"
            placeholder="/tmp/dir or remote:path"
            class="field-input"
            data-testid="ops-file-path"
          />
          <span class="text-[10px] font-normal text-text-dim">
            {{ t('operations.pathHint', { op: selectedOp }) }}
          </span>
        </label>
        <button class="btn-primary" data-testid="ops-file-run" :disabled="store.busy" @click="doFileOp">
          {{ store.busy ? t('common.running') : t('common.run') }}
        </button>
      </div>

      <div
        class="mt-3 rounded-md border px-3 py-2.5"
        :class="riskClass(fileOpMeta.risk)"
        data-testid="ops-file-help"
      >
        <div class="mb-1 flex flex-wrap items-center gap-1.5 text-[12px] font-semibold text-text">
          <PhWarning v-if="fileOpMeta.risk === 'danger' || fileOpMeta.risk === 'caution'" :size="14" weight="bold" />
          <PhInfo v-else :size="14" weight="bold" />
          <span class="font-mono">{{ selectedOp }}</span>
          <span class="font-sans font-normal text-text-muted">· {{ t(`fileOpHelp.${selectedOp}.summary`) }}</span>
          <span
            v-if="fileOpMeta.risk === 'danger'"
            class="rounded bg-danger/15 px-1.5 py-px text-[10px] font-semibold uppercase tracking-wide text-danger"
          >
            {{ t('common.destructive') }}
          </span>
        </div>
        <p class="m-0 text-[12px] leading-relaxed text-text-muted">{{ t(`fileOpHelp.${selectedOp}.detail`) }}</p>
      </div>

      <details class="mt-3">
        <summary class="cursor-pointer text-[11px] font-medium text-text-dim hover:text-text-muted">
          {{ t('operations.allFileGlance') }}
        </summary>
        <dl class="mt-2 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          <div
            v-for="op in FILE_OPS"
            :key="op"
            class="rounded-md border border-border bg-bg px-2.5 py-2"
          >
            <dt class="flex items-center gap-1.5 font-mono text-[12px] font-semibold text-text">
              {{ op }}
              <span
                v-if="FILE_OP_META[op].risk === 'danger'"
                class="rounded bg-danger/15 px-1 py-px text-[9px] font-sans font-semibold uppercase text-danger"
              >
                {{ t('common.danger') }}
              </span>
            </dt>
            <dd class="m-0 mt-0.5 text-[11px] leading-snug text-text-muted">
              {{ t(`fileOpHelp.${op}.summary`) }}. {{ t(`fileOpHelp.${op}.detail`) }}
            </dd>
          </div>
        </dl>
      </details>

      <AppAlert v-if="store.lastOpResult" type="success" class="mt-3" data-testid="ops-file-result">
        {{ store.lastOpResult }}
      </AppAlert>
    </section>
  </div>
</template>
