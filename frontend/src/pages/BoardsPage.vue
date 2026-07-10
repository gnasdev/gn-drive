<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhSquaresFour, PhPlus, PhTrash, PhPlay, PhStop } from '@phosphor-icons/vue'
import { useBoardsStore } from '@/stores/boards'
import { useRemotesStore } from '@/stores/remotes'
import type { Board, BoardEdge, BoardNode } from '@/api/types'
import { useConfirmDialog, useToast } from '@gnas/ui-shared'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'
import AppSectionLoading from '@gnas/ui-shared/components/AppSectionLoading.vue'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'

const store = useBoardsStore()
const remotes = useRemotesStore()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()
const showAdd = ref(false)
const draft = ref<Board>({ id: '', name: '', description: '', created_at: '', updated_at: '', nodes: [], edges: [] })

// Minimal 2-node edge form so execute is usable without a full canvas editor.
const nodeA = ref({ remote_name: '', path: '', label: 'source' })
const nodeB = ref({ remote_name: '', path: '', label: 'target' })
const edgeAction = ref('push')
const msg = ref<string | null>(null)

onMounted(async () => {
  await Promise.all([store.load(), remotes.load()])
})

async function submitAdd() {
  if (!draft.value.name) return
  const id = crypto.randomUUID()
  const n1: BoardNode = {
    id: crypto.randomUUID(),
    remote_name: nodeA.value.remote_name.trim(),
    path: nodeA.value.path.trim() || '/',
    label: nodeA.value.label || 'source',
    x: 0,
    y: 0,
  }
  const n2: BoardNode = {
    id: crypto.randomUUID(),
    remote_name: nodeB.value.remote_name.trim(),
    path: nodeB.value.path.trim() || '/',
    label: nodeB.value.label || 'target',
    x: 200,
    y: 0,
  }
  const edges: BoardEdge[] = []
  if (n1.remote_name || n1.path) {
    if (!(n2.remote_name || n2.path)) {
      toast.error('Target node needs a remote or absolute path')
      return
    }
    edges.push({
      id: crypto.randomUUID(),
      source_id: n1.id,
      target_id: n2.id,
      action: edgeAction.value || 'push',
    })
  }
  const board: Board = {
    ...draft.value,
    id,
    nodes: edges.length ? [n1, n2] : [],
    edges,
  }
  await store.add(board)
  showAdd.value = false
  draft.value = { id: '', name: '', description: '', created_at: '', updated_at: '', nodes: [], edges: [] }
  nodeA.value = { remote_name: '', path: '', label: 'source' }
  nodeB.value = { remote_name: '', path: '', label: 'target' }
  msg.value = null
}

async function doDelete(id: string, name: string) {
  const ok = await confirmDialog({ title: 'Delete board', message: `Delete board "${name}"?`, confirmText: 'Delete', confirmVariant: 'danger' })
  if (!ok) return
  await store.remove(id)
}

async function doExecute(id: string) {
  msg.value = null
  try {
    const r = await store.execute(id, true)
    msg.value = `Board run started (${r?.run_id ?? 'ok'})`
    toast.success('Board execution started')
  } catch (e: any) {
    const m = e?.message ?? 'execute failed'
    msg.value = m
    toast.error(m)
  }
}

async function doStop(id: string) {
  try {
    await store.stop(id)
    toast.success('Board stop requested')
  } catch (e: any) {
    toast.error(e?.message ?? 'stop failed')
  }
}
</script>

<template>
  <div class="boards-page" data-testid="page-boards">
    <header class="page-header">
      <div>
        <h1>Boards</h1>
        <p class="sub">DAG-based multi-step sync workflows.</p>
      </div>
      <button class="primary" data-testid="boards-add" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> Add board
      </button>
    </header>

    <AppAlert v-if="msg" type="info" data-testid="boards-msg">{{ msg }}</AppAlert>
    <AppAlert v-if="store.error" type="error">{{ store.error }}</AppAlert>

    <div v-if="showAdd" class="add-card" data-testid="boards-add-form">
      <h3>New board</h3>
      <form @submit.prevent="submitAdd" class="form-grid">
        <label class="span-2"><span>Name</span><input v-model="draft.name" required data-testid="boards-name" /></label>
        <label class="span-2"><span>Description</span><input v-model="draft.description" data-testid="boards-description" /></label>

        <h4 class="span-2 section">Edge (optional, needed to Execute)</h4>
        <label>
          <span>Source remote</span>
          <select v-model="nodeA.remote_name" data-testid="boards-src-remote">
            <option value="">Local / absolute path</option>
            <option v-for="r in remotes.items" :key="r.name" :value="r.name">{{ r.name }}{{ r.type ? ` (${r.type})` : '' }}</option>
          </select>
        </label>
        <label><span>Source path</span><input v-model="nodeA.path" placeholder="/Backup or abs path" data-testid="boards-src-path" /></label>
        <label>
          <span>Target remote</span>
          <select v-model="nodeB.remote_name" data-testid="boards-dst-remote">
            <option value="">Local / absolute path</option>
            <option v-for="r in remotes.items" :key="r.name" :value="r.name">{{ r.name }}{{ r.type ? ` (${r.type})` : '' }}</option>
          </select>
        </label>
        <label><span>Target path</span><input v-model="nodeB.path" placeholder="/Archive or /tmp/dst" data-testid="boards-dst-path" /></label>
        <label><span>Action</span>
          <select v-model="edgeAction" data-testid="boards-edge-action">
            <option value="push">push</option>
            <option value="pull">pull</option>
            <option value="copy">copy</option>
            <option value="bi">bi</option>
          </select>
        </label>
        <p class="span-2 hint">
          Remote empty + absolute path uses local filesystem. Remote set uses <code>remote:path</code>.
        </p>

        <div class="form-actions">
          <button type="button" class="ghost" @click="showAdd = false">Cancel</button>
          <button type="submit" class="primary" data-testid="boards-submit">Add</button>
        </div>
      </form>
    </div>

    <div class="grid" v-if="store.items.length > 0">
      <div v-for="b in store.items" :key="b.id" class="card" :data-testid="`board-card-${b.id}`">
        <div class="card-head">
          <PhSquaresFour :size="20" weight="light" />
          <div>
            <div class="card-title">{{ b.name }}</div>
            <div class="card-sub">{{ b.description || '(no description)' }}</div>
          </div>
        </div>
        <div class="card-foot">
          <span class="muted small">{{ (b.nodes?.length || 0) }} nodes · {{ (b.edges?.length || 0) }} edges</span>
          <div class="actions">
            <button
              class="ghost small"
              :data-testid="`boards-execute-${b.id}`"
              title="Execute DAG (loads full graph server-side)"
              @click="doExecute(b.id)"
            >
              <PhPlay :size="14" weight="bold" /> Run
            </button>
            <button
              class="ghost small"
              :data-testid="`boards-stop-${b.id}`"
              title="Stop running board"
              @click="doStop(b.id)"
            >
              <PhStop :size="14" weight="bold" /> Stop
            </button>
            <button class="danger small" @click="doDelete(b.id, b.name)"><PhTrash :size="14" weight="regular" /></button>
          </div>
        </div>
        <div v-if="store.lastRun[b.id]" class="run-status muted small">
          run: {{ store.lastRun[b.id].status }} <span v-if="store.lastRun[b.id].run_id">({{ store.lastRun[b.id].run_id }})</span>
        </div>
      </div>
    </div>
    <div v-else-if="!store.loading"><EmptyState title="No boards configured" /></div>
    <div v-else><AppSectionLoading label="Loading boards..." /></div>
  </div>
</template>

<style scoped>
.boards-page { max-width: 1100px; margin: 0 auto; }
.page-header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px; gap: 16px; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.primary { display: inline-flex; align-items: center; gap: 6px; padding: 7px 14px; background: var(--color-accent); color: white; border: 0; border-radius: 6px; font-size: 13px; font-weight: 500; }
.ghost { display: inline-flex; align-items: center; gap: 4px; padding: 6px 10px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-size: 12px; }
.ghost:hover { background: var(--color-surface-hover); }
.ghost.small { padding: 5px 8px; }
.ghost:disabled { opacity: 0.45; cursor: not-allowed; }
.danger { display: inline-flex; padding: 5px 8px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-muted); font-size: 12px; }
.danger:hover { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); border-color: color-mix(in srgb, var(--color-danger) 30%, transparent); }
.danger.small { padding: 5px 8px; }

.add-card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px 20px; margin-bottom: 16px; }
.add-card h3 { margin: 0 0 12px; font-size: 13px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; }
.section { margin: 8px 0 0; font-size: 11px; font-weight: 600; text-transform: uppercase; color: var(--color-text-dim); letter-spacing: 0.4px; }
.form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-items: end; }
.form-grid label { display: flex; flex-direction: column; gap: 4px; }
.form-grid label span { font-size: 11px; color: var(--color-text-muted); font-weight: 500; }
.form-grid input, .form-grid select { padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px; }
.form-grid input:focus, .form-grid select:focus { outline: none; border-color: var(--color-accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent); }
.span-2 { grid-column: 1 / -1; }
.hint { margin: 0; font-size: 11px; color: var(--color-text-dim); }
.hint code { font-family: var(--font-mono); padding: 1px 4px; background: var(--color-bg); border-radius: 3px; }
.form-actions { grid-column: 1 / -1; display: flex; gap: 8px; justify-content: flex-end; }

.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 10px; }
.card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 14px 16px; }
.card-head { display: flex; gap: 10px; align-items: flex-start; color: var(--color-accent); }
.card-title { font-size: 14px; font-weight: 600; color: var(--color-text); }
.card-sub { font-size: 11px; color: var(--color-text-muted); margin-top: 2px; }
.card-foot { display: flex; justify-content: space-between; align-items: center; margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--color-border); gap: 8px; }
.actions { display: flex; gap: 4px; align-items: center; }
.run-status { margin-top: 8px; font-family: var(--font-mono); }
.muted { color: var(--color-text-muted); }
.small { font-size: 11px; }
</style>
