<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhStack, PhPlus, PhTrash } from '@phosphor-icons/vue'
import { useFlowsStore } from '@/stores/flows'
import type { Flow } from '@/api/types'
import { useConfirmDialog } from '@gnas/ui-shared'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'
import AppSectionLoading from '@gnas/ui-shared/components/AppSectionLoading.vue'
import AppCheckbox from '@gnas/ui-shared/components/AppCheckbox.vue'

const store = useFlowsStore()
const { confirmDialog } = useConfirmDialog()
const showAdd = ref(false)
const draft = ref<Flow>({ id: '', name: '', enabled: false })

onMounted(() => store.load())

async function submitAdd() {
  if (!draft.value.name) return
  await store.add({ ...draft.value, id: crypto.randomUUID() })
  showAdd.value = false
  draft.value = { id: '', name: '', enabled: false }
}

async function doDelete(id: string, name: string) {
  const ok = await confirmDialog({ title: 'Delete flow', message: `Delete flow "${name}"?`, confirmText: 'Delete', confirmVariant: 'danger' })
  if (!ok) return
  await store.remove(id)
}
</script>

<template>
  <div class="flows-page" data-testid="page-flows">
    <header class="page-header">
      <div>
        <h1>Flows</h1>
        <p class="sub">Sequential multi-step sync flows.</p>
      </div>
      <button class="primary" data-testid="flows-add" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> Add flow
      </button>
    </header>

    <div v-if="showAdd" class="add-card" data-testid="flows-add-form">
      <h3>New flow</h3>
      <form @submit.prevent="submitAdd" class="form-grid">
        <label class="span-2"><span>Name</span><input v-model="draft.name" required data-testid="flows-name" /></label>
        <label><span>Cron optional (5-field)</span><input v-model="draft.schedule_cron" placeholder="0 * * * *" data-testid="flows-cron" /></label>
        <label class="checkbox">
          <AppCheckbox v-model="draft.enabled" label="Enabled" />
        </label>
        <div class="form-actions">
          <button type="button" class="ghost" @click="showAdd = false">Cancel</button>
          <button type="submit" class="primary" data-testid="flows-submit">Add</button>
        </div>
      </form>
    </div>

    <div class="grid" v-if="store.items.length > 0">
      <div v-for="f in store.items" :key="f.id" class="card">
        <div class="card-head">
          <PhStack :size="20" weight="light" />
          <div>
            <div class="card-title">{{ f.name }}</div>
            <div class="card-sub">
              {{ f.operations?.length || 0 }} operations
              <span v-if="f.schedule_cron" class="muted"> · cron: <code class="mono">{{ f.schedule_cron }}</code></span>
            </div>
          </div>
        </div>
        <div class="card-foot">
          <span :class="['status', f.enabled ? 'on' : 'off']">{{ f.enabled ? 'enabled' : 'disabled' }}</span>
          <button class="danger small" @click="doDelete(f.id, f.name)"><PhTrash :size="14" weight="regular" /></button>
        </div>
      </div>
    </div>
    <div v-else-if="!store.loading"><EmptyState title="No flows configured" /></div>
    <div v-else><AppSectionLoading label="Loading flows..." /></div>
  </div>
</template>

<style scoped>
.flows-page { max-width: 1100px; margin: 0 auto; }
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
.checkbox { flex-direction: row !important; align-items: center; gap: 8px !important; }
.checkbox input { width: auto; }
.form-actions { grid-column: 1 / -1; display: flex; gap: 8px; justify-content: flex-end; }

.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 10px; }
.card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 14px 16px; }
.card-head { display: flex; gap: 10px; align-items: flex-start; color: var(--color-accent); }
.card-title { font-size: 14px; font-weight: 600; color: var(--color-text); }
.card-sub { font-size: 11px; color: var(--color-text-muted); margin-top: 2px; }
.card-sub .mono { font-family: var(--font-mono); }
.card-foot { display: flex; justify-content: space-between; align-items: center; margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--color-border); }
.muted { color: var(--color-text-muted); }
.status { font-size: 11px; padding: 1px 6px; border-radius: 3px; }
.status.on { background: color-mix(in srgb, var(--color-success) 18%, transparent); color: var(--color-success); }
.status.off { background: var(--color-surface-hover); color: var(--color-text-muted); }
.empty { color: var(--color-text-dim); text-align: center; padding: 40px; }
.loading { padding: 24px; color: var(--color-text-muted); text-align: center; }
</style>
