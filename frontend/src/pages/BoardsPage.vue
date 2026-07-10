<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhSquaresFour, PhPlus, PhTrash, PhPlay, PhStop } from '@phosphor-icons/vue'
import { useBoardsStore } from '@/stores/boards'
import { useRemotesStore } from '@/stores/remotes'
import type { Board, BoardEdge, BoardNode } from '@/api/types'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { useToast } from '@/composables/useToast'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSectionLoading from '@/components/ui/SectionLoading.vue'
import AppAlert from '@/components/ui/Alert.vue'

const { t } = useI18n()
const store = useBoardsStore()
const remotes = useRemotesStore()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()
const showAdd = ref(false)
const draft = ref<Board>({ id: '', name: '', description: '', created_at: '', updated_at: '', nodes: [], edges: [] })

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
      toast.error(t('boards.targetRequired'))
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
  const ok = await confirmDialog({
    title: t('boards.deleteTitle'),
    message: t('boards.deleteMessage', { name }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await store.remove(id)
}

async function doExecute(id: string) {
  msg.value = null
  try {
    const r = await store.execute(id, true)
    msg.value = t('boards.runStarted', { id: r?.run_id ?? 'ok' })
    toast.success(t('boards.execStarted'))
  } catch (e: any) {
    const m = e?.message ?? 'execute failed'
    msg.value = m
    toast.error(m)
  }
}

async function doStop(id: string) {
  try {
    await store.stop(id)
    toast.success(t('boards.stopRequested'))
  } catch (e: any) {
    toast.error(e?.message ?? 'stop failed')
  }
}
</script>

<template>
  <div class="page-shell" data-testid="page-boards">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('boards.title') }}</h1>
        <p class="page-sub">{{ t('boards.sub') }}</p>
      </div>
      <button class="btn-primary" data-testid="boards-add" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> {{ t('boards.add') }}
      </button>
    </header>

    <AppAlert v-if="msg" type="info" data-testid="boards-msg">{{ msg }}</AppAlert>
    <AppAlert v-if="store.error" type="error">{{ store.error }}</AppAlert>

    <div v-if="showAdd" class="card mb-4 px-5 py-4" data-testid="boards-add-form">
      <h3 class="section-label">{{ t('boards.new') }}</h3>
      <form class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-end" @submit.prevent="submitAdd">
        <label class="field-label md:col-span-2">
          <span>{{ t('common.name') }}</span>
          <input v-model="draft.name" required class="field-input" data-testid="boards-name" />
        </label>
        <label class="field-label md:col-span-2">
          <span>{{ t('common.description') }}</span>
          <input v-model="draft.description" class="field-input" data-testid="boards-description" />
        </label>

        <h4 class="md:col-span-2 m-0 text-[11px] font-semibold uppercase tracking-wide text-text-dim">
          {{ t('boards.edgeSection') }}
        </h4>
        <label class="field-label">
          <span>{{ t('boards.sourceRemote') }}</span>
          <select v-model="nodeA.remote_name" class="field-input" data-testid="boards-src-remote">
            <option value="">{{ t('common.localAbsolute') }}</option>
            <option v-for="r in remotes.items" :key="r.name" :value="r.name">
              {{ r.name }}{{ r.type ? ` (${r.type})` : '' }}
            </option>
          </select>
        </label>
        <label class="field-label">
          <span>{{ t('boards.sourcePath') }}</span>
          <input v-model="nodeA.path" placeholder="/Backup or abs path" class="field-input" data-testid="boards-src-path" />
        </label>
        <label class="field-label">
          <span>{{ t('boards.targetRemote') }}</span>
          <select v-model="nodeB.remote_name" class="field-input" data-testid="boards-dst-remote">
            <option value="">{{ t('common.localAbsolute') }}</option>
            <option v-for="r in remotes.items" :key="r.name" :value="r.name">
              {{ r.name }}{{ r.type ? ` (${r.type})` : '' }}
            </option>
          </select>
        </label>
        <label class="field-label">
          <span>{{ t('boards.targetPath') }}</span>
          <input v-model="nodeB.path" placeholder="/Archive or /tmp/dst" class="field-input" data-testid="boards-dst-path" />
        </label>
        <label class="field-label">
          <span>{{ t('common.action') }}</span>
          <select v-model="edgeAction" class="field-input" data-testid="boards-edge-action">
            <option value="push">push</option>
            <option value="pull">pull</option>
            <option value="copy">copy</option>
            <option value="bi">bi</option>
          </select>
        </label>
        <p class="md:col-span-2 m-0 text-[11px] text-text-dim">
          <i18n-t keypath="boards.edgeHint" tag="span">
            <template #path>
              <code class="rounded bg-bg px-1 font-mono">remote:path</code>
            </template>
          </i18n-t>
        </p>

        <div class="flex justify-end gap-2 md:col-span-2">
          <button type="button" class="btn-ghost" @click="showAdd = false">{{ t('common.cancel') }}</button>
          <button type="submit" class="btn-primary" data-testid="boards-submit">{{ t('common.add') }}</button>
        </div>
      </form>
    </div>

    <div v-if="store.items.length > 0" class="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-2.5">
      <div v-for="b in store.items" :key="b.id" class="card p-4" :data-testid="`board-card-${b.id}`">
        <div class="flex items-start gap-2.5 text-accent">
          <PhSquaresFour :size="20" weight="light" />
          <div>
            <div class="text-sm font-semibold text-text">{{ b.name }}</div>
            <div class="mt-0.5 text-[11px] text-text-muted">{{ b.description || t('boards.noDescription') }}</div>
          </div>
        </div>
        <div class="mt-3 flex items-center justify-between gap-2 border-t border-border pt-2.5">
          <span class="text-[11px] text-text-muted">
            {{ t('boards.nodesEdges', { nodes: b.nodes?.length || 0, edges: b.edges?.length || 0 }) }}
          </span>
          <div class="flex items-center gap-1">
            <button
              class="btn-ghost !px-2 !py-1"
              :data-testid="`boards-execute-${b.id}`"
              @click="doExecute(b.id)"
            >
              <PhPlay :size="14" weight="bold" /> {{ t('boards.execute') }}
            </button>
            <button
              class="btn-ghost !px-2 !py-1"
              :data-testid="`boards-stop-${b.id}`"
              @click="doStop(b.id)"
            >
              <PhStop :size="14" weight="bold" /> {{ t('boards.stop') }}
            </button>
            <button class="danger !p-1.5" :data-testid="`boards-delete-${b.id}`" @click="doDelete(b.id, b.name)">
              <PhTrash :size="14" weight="regular" />
            </button>
          </div>
        </div>
        <div v-if="store.lastRun[b.id]" class="mt-2 font-mono text-[11px] text-text-muted">
          {{ t('boards.runStatus', { status: store.lastRun[b.id].status }) }}
          <span v-if="store.lastRun[b.id].run_id">({{ store.lastRun[b.id].run_id }})</span>
        </div>
      </div>
    </div>
    <div v-else-if="!store.loading"><EmptyState :title="t('boards.empty')" /></div>
    <div v-else><AppSectionLoading :label="t('boards.loading')" /></div>
  </div>
</template>
