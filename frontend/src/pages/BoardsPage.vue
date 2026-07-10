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
import RemotePathField from '@/components/forms/RemotePathField.vue'
import {
  BOARD_EDGE_ACTIONS,
  composedPathToBoardNode,
} from '@/constants/forms'

const { t } = useI18n()
const store = useBoardsStore()
const remotes = useRemotesStore()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()
const showAdd = ref(false)
const name = ref('')
const sourcePath = ref('')
const targetPath = ref('')
const edgeAction = ref('copy')
const msg = ref<string | null>(null)

onMounted(async () => {
  await Promise.all([store.load(), remotes.load()])
})

function resetForm() {
  name.value = ''
  sourcePath.value = ''
  targetPath.value = ''
  edgeAction.value = 'copy'
}

async function submitAdd() {
  if (!name.value.trim()) {
    toast.error(t('boards.nameRequired'))
    return
  }
  if (!sourcePath.value.trim() || !targetPath.value.trim()) {
    toast.error(t('boards.pathsRequired'))
    return
  }
  const src = composedPathToBoardNode(sourcePath.value)
  const dst = composedPathToBoardNode(targetPath.value)
  if (!src.path && !src.remote_name) {
    toast.error(t('boards.pathsRequired'))
    return
  }
  if (!dst.path && !dst.remote_name) {
    toast.error(t('boards.targetRequired'))
    return
  }

  const id = crypto.randomUUID()
  const n1: BoardNode = {
    id: crypto.randomUUID(),
    remote_name: src.remote_name,
    path: src.path || '/',
    label: 'source',
    x: 0,
    y: 0,
  }
  const n2: BoardNode = {
    id: crypto.randomUUID(),
    remote_name: dst.remote_name,
    path: dst.path || '/',
    label: 'target',
    x: 200,
    y: 0,
  }
  const edges: BoardEdge[] = [
    {
      id: crypto.randomUUID(),
      source_id: n1.id,
      target_id: n2.id,
      action: edgeAction.value || 'copy',
    },
  ]
  const board: Board = {
    id,
    name: name.value.trim(),
    created_at: '',
    updated_at: '',
    nodes: [n1, n2],
    edges,
  }
  await store.add(board)
  showAdd.value = false
  resetForm()
  msg.value = null
}

async function doDelete(id: string, boardName: string) {
  const ok = await confirmDialog({
    title: t('boards.deleteTitle'),
    message: t('boards.deleteMessage', { name: boardName }),
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

function edgeSummary(b: Board): string {
  const e = b.edges?.[0]
  if (!e) return t('boards.noEdge')
  return e.action || '-'
}
</script>

<template>
  <div class="page-shell" data-testid="page-boards">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('boards.title') }}</h1>
        <p class="page-sub">{{ t('boards.sub') }}</p>
      </div>
      <button class="btn-primary" data-testid="boards-add" @click="showAdd = !showAdd; if (!showAdd) resetForm()">
        <PhPlus :size="16" weight="bold" /> {{ t('boards.add') }}
      </button>
    </header>

    <AppAlert v-if="msg" type="info" data-testid="boards-msg">{{ msg }}</AppAlert>
    <AppAlert v-if="store.error" type="error">{{ store.error }}</AppAlert>

    <div v-if="showAdd" class="card mb-4 px-5 py-4" data-testid="boards-add-form">
      <h3 class="section-label">{{ t('boards.new') }}</h3>
      <form class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-start" @submit.prevent="submitAdd">
        <label class="field-label md:col-span-2">
          <span>{{ t('common.name') }}</span>
          <input v-model="name" required class="field-input" data-testid="boards-name" />
        </label>

        <div class="md:col-span-2">
          <RemotePathField
            v-model="sourcePath"
            :remotes="remotes.items"
            test-id="boards-src-path"
            :label="t('boards.source')"
            required
          />
        </div>
        <div class="md:col-span-2">
          <RemotePathField
            v-model="targetPath"
            :remotes="remotes.items"
            test-id="boards-dst-path"
            :label="t('boards.target')"
            required
          />
        </div>

        <label class="field-label">
          <span>{{ t('common.action') }}</span>
          <select v-model="edgeAction" class="field-input" data-testid="boards-edge-action">
            <option v-for="a in BOARD_EDGE_ACTIONS" :key="a" :value="a">{{ a }}</option>
          </select>
        </label>
        <p class="m-0 self-end text-[11px] text-text-dim md:col-span-1">
          {{ t('boards.actionHint') }}
        </p>

        <div class="flex justify-end gap-2 md:col-span-2">
          <button type="button" class="btn-ghost" @click="showAdd = false; resetForm()">{{ t('common.cancel') }}</button>
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
          </div>
        </div>
        <div class="mt-3 flex items-center justify-between gap-2 border-t border-border pt-2.5">
          <span class="text-[11px] text-text-muted">
            {{ t('boards.nodesEdges', { nodes: b.nodes?.length || 0, edges: b.edges?.length || 0 }) }}
            · <span class="badge">{{ edgeSummary(b) }}</span>
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
