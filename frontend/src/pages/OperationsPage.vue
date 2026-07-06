<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhSwap, PhPlay, PhFolder, PhFile, PhArrowLeft } from '@phosphor-icons/vue'
import { useOperationsStore } from '@/stores/operations'
import { useApi } from '@/composables/useApi'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'

const store = useOperationsStore()
const api = useApi()

const remoteInput = ref('gdrive:/')
const lastTaskId = ref<string | null>(null)

onMounted(async () => {
  await store.loadProfiles()
  await store.loadTasks()
})

async function doBrowse() {
  if (!remoteInput.value) return
  await store.browse(remoteInput.value)
}

async function doStartSync(action: 'pull' | 'push' | 'bi' | 'bi-resync') {
  const profileName = prompt(`Profile name to ${action}?`)
  if (!profileName) return
  lastTaskId.value = await store.startSync(action, profileName)
  if (lastTaskId.value) {
    setTimeout(() => store.loadTasks(), 500)
  }
}
</script>

<template>
  <div class="ops-page">
    <header class="page-header">
      <div>
        <h1>Operations</h1>
        <p class="sub">Browse remotes and trigger one-shot syncs.</p>
      </div>
    </header>

    <section class="card">
      <h3>Quick sync</h3>
      <p class="row-help">Trigger a one-shot sync for a profile.</p>
      <div class="quick-actions">
        <button class="ghost" @click="doStartSync('pull')"><PhPlay :size="14" weight="bold" /> pull</button>
        <button class="ghost" @click="doStartSync('push')"><PhPlay :size="14" weight="bold" /> push</button>
        <button class="ghost" @click="doStartSync('bi')"><PhPlay :size="14" weight="bold" /> bi</button>
        <button class="ghost" @click="doStartSync('bi-resync')"><PhPlay :size="14" weight="bold" /> bi-resync</button>
      </div>
      <AppAlert v-if="lastTaskId" type="success">Started task <code>{{ lastTaskId }}</code></AppAlert>
    </section>

    <section class="card">
      <h3>Active tasks</h3>
      <div v-if="store.tasks.length === 0"><EmptyState title="No active tasks" /></div>
      <div v-else class="task-list">
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
        <input v-model="remoteInput" placeholder="remote:/path" @keydown.enter="doBrowse" />
        <button class="primary" @click="doBrowse" :disabled="store.busy">{{ store.busy ? 'Loading…' : 'Browse' }}</button>
      </div>
      <AppAlert v-if="store.error" type="error">{{ store.error }}</AppAlert>
      <p class="row-help">
        Note: file browser requires the backend's <code>/api/v1/operations/fs</code> endpoint
        to be implemented. Currently returns 501 in Phase 3.
      </p>
      <div v-if="store.entries.length > 0" class="file-list">
        <div class="file-row head">
          <span>Name</span>
          <span>Size</span>
          <span>Modified</span>
        </div>
        <div v-for="e in store.entries" :key="e.path" class="file-row">
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

.browse-bar { display: flex; gap: 6px; margin-bottom: 8px; }
.browse-bar input { flex: 1; padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px; }
.browse-bar input:focus { outline: none; border-color: var(--color-accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent); }
.primary { padding: 7px 14px; background: var(--color-accent); color: white; border: 0; border-radius: 6px; font-size: 13px; font-weight: 500; }
.primary:disabled { opacity: 0.5; }

.file-list { background: var(--color-bg); border-radius: 6px; overflow: hidden; }
.file-row { display: grid; grid-template-columns: 1fr 100px 180px; gap: 12px; padding: 6px 12px; font-size: 12px; border-top: 1px solid var(--color-border); align-items: center; }
.file-row.head { font-size: 10px; color: var(--color-text-dim); text-transform: uppercase; letter-spacing: 0.4px; border-top: 0; padding: 6px 12px; }
.file-row span:first-child { display: flex; align-items: center; gap: 6px; }
</style>
