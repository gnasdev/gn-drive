<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { PhPlay, PhFolder, PhFile } from '@phosphor-icons/vue'
import { useOperationsStore, type FileOp } from '@/stores/operations'
import { useRemotesStore } from '@/stores/remotes'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'
import { useToast } from '@gnas/ui-shared'
import { SYNC_ACTIONS } from '@/constants/forms'

const store = useOperationsStore()
const remotes = useRemotesStore()
const toast = useToast()

/** '' = free absolute / custom path in remoteInput */
const selectedRemote = ref('')
const remoteInput = ref('/')
const lastTaskId = ref<string | null>(null)
const opSource = ref('')
const opDest = ref('')
const opPath = ref('')
const selectedOp = ref<FileOp>('copy')
const syncProfile = ref('')

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

async function doStartSync(action: (typeof SYNC_ACTIONS)[number]) {
  const profileName = syncProfile.value || store.profiles[0]?.name
  if (!profileName) {
    toast.error('Create a profile first')
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
</script>

<template>
  <div class="ops-page" data-testid="page-operations">
    <header class="page-header">
      <div>
        <h1>Operations</h1>
        <p class="sub">Browse remotes and trigger one-shot syncs.</p>
      </div>
    </header>

    <section class="card">
      <h3>Quick sync</h3>
      <p class="row-help">Trigger a one-shot sync for a profile.</p>
      <div class="browse-bar" style="margin-bottom: 10px">
        <select v-model="syncProfile" data-testid="ops-sync-profile">
          <option value="" disabled>Select profile</option>
          <option v-for="p in store.profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
      </div>
      <div class="quick-actions">
        <button
          v-for="a in SYNC_ACTIONS"
          :key="a"
          class="ghost"
          :data-testid="`ops-sync-${a}`"
          @click="doStartSync(a)"
        >
          <PhPlay :size="14" weight="bold" /> {{ a }}
        </button>
      </div>
      <AppAlert v-if="lastTaskId" type="success" data-testid="ops-task-started">Started task <code>{{ lastTaskId }}</code></AppAlert>
    </section>

    <section class="card">
      <h3>Active tasks</h3>
      <div v-if="store.tasks.length === 0"><EmptyState title="No active tasks" /></div>
      <div v-else class="task-list" data-testid="ops-task-list">
        <div v-for="t in store.tasks" :key="t.id" class="task">
          <div class="task-name">{{ t.name }} <span class="badge">{{ t.action }}</span></div>
          <div class="task-meta">
            <span :class="['status', t.status]">{{ t.status }}</span>
            <span v-if="t.transferred" class="mono small">{{ t.transferred }} B</span>
          </div>
        </div>
      </div>
    </section>

    <section class="card">
      <h3>Browse remote</h3>
      <div class="browse-bar">
        <select v-model="selectedRemote" data-testid="ops-browse-remote">
          <option value="">Custom / absolute</option>
          <option v-for="r in remotes.items" :key="r.name" :value="r.name">{{ r.name }}{{ r.type ? ` (${r.type})` : '' }}</option>
        </select>
        <input v-model="remoteInput" placeholder="remote:/path or /absolute" data-testid="ops-browse-path" @keydown.enter="doBrowse" />
        <button class="primary" data-testid="ops-browse-submit" @click="doBrowse" :disabled="store.busy">{{ store.busy ? 'Loading…' : 'Browse' }}</button>
      </div>
      <AppAlert v-if="store.error" type="error" data-testid="ops-browse-error">{{ store.error }}</AppAlert>
      <p class="row-help">
        Pick a remote or enter an absolute local path (<code>/tmp</code>) or <code>remote:path</code>.
      </p>
      <div v-if="store.entries.length > 0" class="file-list" data-testid="ops-browse-list">
        <div class="file-row head">
          <span>Name</span>
          <span>Size</span>
          <span>Modified</span>
        </div>
        <div v-for="e in store.entries" :key="e.path || e.name" class="file-row" data-testid="ops-browse-row">
          <span>
            <PhFolder v-if="e.is_dir" :size="14" weight="regular" />
            <PhFile v-else :size="14" weight="regular" />
            <span class="mono small">{{ e.name }}</span>
          </span>
          <span class="mono small">{{ e.is_dir ? '—' : e.size + ' B' }}</span>
          <span class="muted small">{{ e.mod_time }}</span>
        </div>
      </div>
    </section>

    <section class="card">
      <h3>File operations</h3>
      <p class="row-help">One-shot rclone ops: copy, move, check, mkdir, purge, delete.</p>
      <div class="op-form">
        <label>
          <span>Op</span>
          <select v-model="selectedOp" data-testid="ops-file-op">
            <option value="copy">copy</option>
            <option value="move">move</option>
            <option value="check">check</option>
            <option value="mkdir">mkdir</option>
            <option value="purge">purge</option>
            <option value="delete">delete</option>
          </select>
        </label>
        <label v-if="selectedOp === 'copy' || selectedOp === 'move' || selectedOp === 'check'">
          <span>Source</span>
          <input v-model="opSource" placeholder="/tmp/src or remote:path" data-testid="ops-file-source" />
        </label>
        <label v-if="selectedOp === 'copy' || selectedOp === 'move' || selectedOp === 'check'">
          <span>Dest</span>
          <input v-model="opDest" placeholder="/tmp/dst or remote:path" data-testid="ops-file-dest" />
        </label>
        <label v-if="selectedOp === 'mkdir' || selectedOp === 'purge' || selectedOp === 'delete'">
          <span>Path</span>
          <input v-model="opPath" placeholder="/tmp/dir or remote:path" data-testid="ops-file-path" />
        </label>
        <button class="primary" data-testid="ops-file-run" :disabled="store.busy" @click="doFileOp">
          {{ store.busy ? 'Running…' : 'Run' }}
        </button>
      </div>
      <AppAlert v-if="store.lastOpResult" type="success" data-testid="ops-file-result">{{ store.lastOpResult }}</AppAlert>
    </section>
  </div>
</template>

<style scoped>
.ops-page { max-width: 1100px; margin: 0 auto; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0 0 20px; }
.card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px 20px; margin-bottom: 12px; }
.card h3 { font-size: 12px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; margin: 0 0 8px; }
.row-help { font-size: 12px; color: var(--color-text-dim); margin: 0 0 8px; }
.row-help code { font-family: var(--font-mono); padding: 1px 4px; background: var(--color-bg); border-radius: 3px; }

.quick-actions { display: flex; gap: 6px; flex-wrap: wrap; }
.ghost { display: inline-flex; align-items: center; gap: 4px; padding: 5px 10px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-size: 12px; font-family: var(--font-mono); }
.ghost:hover { background: var(--color-surface-hover); }

.banner { padding: 6px 10px; border-radius: 6px; font-size: 12px; margin-top: 8px; }
.banner.ok { background: color-mix(in srgb, var(--color-success) 12%, transparent); color: var(--color-success); }
.banner.err { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); }
.banner code { font-family: var(--font-mono); padding: 0 4px; }

.task-list { display: flex; flex-direction: column; gap: 6px; }
.task { display: flex; justify-content: space-between; align-items: center; padding: 8px 10px; background: var(--color-bg); border-radius: 6px; }
.task-name { font-size: 13px; }
.badge { display: inline-block; margin-left: 4px; padding: 1px 5px; background: var(--color-surface-hover); border-radius: 3px; font-size: 10px; font-family: var(--font-mono); color: var(--color-text-muted); }
.task-meta { display: flex; gap: 8px; align-items: center; }
.status { font-size: 11px; padding: 1px 6px; border-radius: 3px; }
.status.running { background: color-mix(in srgb, var(--color-running) 18%, transparent); color: var(--color-running); }
.status.completed { background: color-mix(in srgb, var(--color-completed) 18%, transparent); color: var(--color-completed); }
.status.failed { background: color-mix(in srgb, var(--color-failed) 18%, transparent); color: var(--color-failed); }
.status.cancelled { background: color-mix(in srgb, var(--color-warning) 18%, transparent); color: var(--color-warning); }
.mono { font-family: var(--font-mono); }
.small { font-size: 11px; }
.muted { color: var(--color-text-muted); }
.empty { color: var(--color-text-dim); font-size: 12px; padding: 8px 0; }

.browse-bar { display: flex; gap: 6px; margin-bottom: 8px; flex-wrap: wrap; }
.browse-bar input, .browse-bar select {
  padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border);
  border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px;
}
.browse-bar input { flex: 1; min-width: 160px; }
.browse-bar select { min-width: 140px; }
.browse-bar input:focus, .browse-bar select:focus {
  outline: none; border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent);
}
.primary { padding: 7px 14px; background: var(--color-accent); color: white; border: 0; border-radius: 6px; font-size: 13px; font-weight: 500; }
.primary:disabled { opacity: 0.5; }

.file-list { background: var(--color-bg); border-radius: 6px; overflow: hidden; }
.file-row { display: grid; grid-template-columns: 1fr 100px 180px; gap: 12px; padding: 6px 12px; font-size: 12px; border-top: 1px solid var(--color-border); align-items: center; }
.file-row.head { font-size: 10px; color: var(--color-text-dim); text-transform: uppercase; letter-spacing: 0.4px; border-top: 0; padding: 6px 12px; }
.file-row span:first-child { display: flex; align-items: center; gap: 6px; }

.op-form { display: grid; grid-template-columns: 120px 1fr 1fr auto; gap: 10px; align-items: end; }
.op-form label { display: flex; flex-direction: column; gap: 4px; }
.op-form label span { font-size: 11px; color: var(--color-text-muted); font-weight: 500; }
.op-form input, .op-form select {
  padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border);
  border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px;
}
@media (max-width: 800px) {
  .op-form { grid-template-columns: 1fr; }
}
</style>
