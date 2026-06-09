<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhSquaresFour, PhPlus, PhTrash } from '@phosphor-icons/vue'
import { useBoardsStore } from '@/stores/boards'
import type { Board } from '@/api/types'

const store = useBoardsStore()
const showAdd = ref(false)
const draft = ref<Board>({ id: '', name: '', description: '', created_at: '', updated_at: '', nodes: [], edges: [] })

onMounted(() => store.load())

async function submitAdd() {
  if (!draft.value.name) return
  await store.add({ ...draft.value, id: crypto.randomUUID() })
  showAdd.value = false
  draft.value = { id: '', name: '', description: '', created_at: '', updated_at: '', nodes: [], edges: [] }
}

async function doDelete(id: string, name: string) {
  if (!confirm(`Delete board "${name}"?`)) return
  await store.remove(id)
}
</script>

<template>
  <div class="boards-page">
    <header class="page-header">
      <div>
        <h1>Boards</h1>
        <p class="sub">DAG-based multi-step sync workflows.</p>
      </div>
      <button class="primary" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> Add board
      </button>
    </header>

    <div v-if="showAdd" class="add-card">
      <h3>New board</h3>
      <form @submit.prevent="submitAdd" class="form-grid">
        <label class="span-2"><span>Name</span><input v-model="draft.name" required /></label>
        <label class="span-2"><span>Description</span><input v-model="draft.description" /></label>
        <div class="form-actions">
          <button type="button" class="ghost" @click="showAdd = false">Cancel</button>
          <button type="submit" class="primary">Add</button>
        </div>
      </form>
    </div>

    <div class="grid" v-if="store.items.length > 0">
      <div v-for="b in store.items" :key="b.id" class="card">
        <div class="card-head">
          <PhSquaresFour :size="20" weight="light" />
          <div>
            <div class="card-title">{{ b.name }}</div>
            <div class="card-sub">{{ b.description || '(no description)' }}</div>
          </div>
        </div>
        <div class="card-foot">
          <span class="muted small">{{ (b.nodes?.length || 0) }} nodes · {{ (b.edges?.length || 0) }} edges</span>
          <button class="danger small" @click="doDelete(b.id, b.name)"><PhTrash :size="14" weight="regular" /></button>
        </div>
      </div>
    </div>
    <div v-else-if="!store.loading" class="empty">No boards configured.</div>
    <div v-else class="loading">Loading…</div>
  </div>
</template>

<style scoped>
.boards-page { max-width: 1100px; margin: 0 auto; }
.page-header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px; gap: 16px; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.primary { display: inline-flex; align-items: center; gap: 6px; padding: 7px 14px; background: var(--color-accent); color: white; border: 0; border-radius: 6px; font-size: 13px; font-weight: 500; }
.ghost { padding: 6px 10px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-size: 12px; }
.danger { display: inline-flex; padding: 5px 8px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-muted); font-size: 12px; }
.danger:hover { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); border-color: color-mix(in srgb, var(--color-danger) 30%, transparent); }
.danger.small { padding: 5px 8px; }

.add-card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px 20px; margin-bottom: 16px; }
.add-card h3 { margin: 0 0 12px; font-size: 13px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; }
.form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-items: end; }
.form-grid label { display: flex; flex-direction: column; gap: 4px; }
.form-grid label span { font-size: 11px; color: var(--color-text-muted); font-weight: 500; }
.form-grid input { padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px; }
.form-grid input:focus { outline: none; border-color: var(--color-accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent); }
.span-2 { grid-column: 1 / -1; }
.form-actions { grid-column: 1 / -1; display: flex; gap: 8px; justify-content: flex-end; }

.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 10px; }
.card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 14px 16px; }
.card-head { display: flex; gap: 10px; align-items: flex-start; color: var(--color-accent); }
.card-title { font-size: 14px; font-weight: 600; color: var(--color-text); }
.card-sub { font-size: 11px; color: var(--color-text-muted); margin-top: 2px; }
.card-foot { display: flex; justify-content: space-between; align-items: center; margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--color-border); }
.muted { color: var(--color-text-muted); }
.small { font-size: 11px; }
.empty { color: var(--color-text-dim); text-align: center; padding: 40px; }
.loading { padding: 24px; color: var(--color-text-muted); text-align: center; }
</style>
