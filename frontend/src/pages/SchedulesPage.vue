<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhCalendar, PhPlus, PhTrash, PhPause, PhPlay } from '@phosphor-icons/vue'
import { useSchedulesStore } from '@/stores/schedules'
import { useApi } from '@/composables/useApi'
import type { Schedule } from '@/api/types'
import { useConfirmDialog, useToast } from '@gnas/ui-shared'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'
import AppSectionLoading from '@gnas/ui-shared/components/AppSectionLoading.vue'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'

const store = useSchedulesStore()
const api = useApi()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()

const showAdd = ref(false)
const draft = ref<Schedule>({ id: '', profile_name: '', action: 'pull', cron: '0 * * * *', enabled: true })

onMounted(() => store.load())

async function submitAdd() {
  if (!draft.value.profile_name || !draft.value.cron) {
    toast.error('Profile and cron are required')
    return
  }
  await store.add({ ...draft.value, id: crypto.randomUUID() })
  showAdd.value = false
  draft.value = { id: '', profile_name: '', action: 'pull', cron: '0 * * * *', enabled: true }
}

async function doDelete(id: string) {
  const ok = await confirmDialog({ title: 'Delete schedule', message: 'Delete this schedule?', confirmText: 'Delete', confirmVariant: 'danger' })
  if (!ok) return
  await store.remove(id)
}
</script>

<template>
  <div class="schedules-page">
    <header class="page-header">
      <div>
        <h1>Schedules</h1>
        <p class="sub">Cron-based scheduled syncs.</p>
      </div>
      <button class="primary" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> Add schedule
      </button>
    </header>

    <div v-if="showAdd" class="add-card">
      <h3>New schedule</h3>
      <form @submit.prevent="submitAdd" class="form-grid">
        <label class="span-2"><span>Profile name</span>
          <input v-model="draft.profile_name" placeholder="backup" required />
        </label>
        <label><span>Action</span>
          <select v-model="draft.action">
            <option value="pull">pull</option>
            <option value="push">push</option>
            <option value="bi">bi</option>
            <option value="bi-resync">bi-resync</option>
          </select>
        </label>
        <label><span>Cron (5-field)</span>
          <input v-model="draft.cron" placeholder="0 * * * *" required />
        </label>
        <div class="form-actions">
          <button type="button" class="ghost" @click="showAdd = false">Cancel</button>
          <button type="submit" class="primary" :disabled="api.loading.value">
            {{ api.loading.value ? 'Adding…' : 'Add' }}
          </button>
        </div>
      </form>
    </div>

    <AppAlert v-if="api.error.value" type="error">{{ api.error.value }}</AppAlert>

    <div class="table-wrap" v-if="store.items.length > 0 || !store.loading">
      <table>
        <thead>
          <tr><th>Profile</th><th>Action</th><th>Cron</th><th>Last run</th><th>Next run</th><th>Enabled</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="s in store.items" :key="s.id">
            <td>
              <div class="cell-name">
                <PhCalendar :size="14" weight="regular" />
                <span class="mono">{{ s.profile_name }}</span>
              </div>
            </td>
            <td><span class="badge">{{ s.action }}</span></td>
            <td class="mono small">{{ s.cron }}</td>
            <td class="muted small">{{ s.last_run || '—' }}</td>
            <td class="muted small">{{ s.next_run || '—' }}</td>
            <td>
              <button class="toggle" :class="{ on: s.enabled }" @click="s.enabled ? store.disable(s.id) : store.enable(s.id)">
                <PhPause v-if="s.enabled" :size="14" weight="bold" />
                <PhPlay v-else :size="14" weight="bold" />
                <span>{{ s.enabled ? 'enabled' : 'disabled' }}</span>
              </button>
            </td>
            <td class="actions">
              <button class="danger small" @click="doDelete(s.id)"><PhTrash :size="14" weight="regular" /></button>
            </td>
          </tr>
          <tr v-if="store.items.length === 0 && !store.loading">
            <td colspan="7" class="empty"><EmptyState title="No schedules configured" /></td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else><AppSectionLoading /></div>
  </div>
</template>

<style scoped>
.schedules-page { max-width: 1200px; margin: 0 auto; }
.page-header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px; gap: 16px; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.primary { display: inline-flex; align-items: center; gap: 6px; padding: 7px 14px; background: var(--color-accent); color: white; border: 0; border-radius: 6px; font-size: 13px; font-weight: 500; }
.ghost { padding: 6px 10px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-size: 12px; }
.ghost:hover { background: var(--color-surface-hover); }
.danger { display: inline-flex; padding: 5px 8px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-muted); font-size: 12px; }
.danger:hover { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); border-color: color-mix(in srgb, var(--color-danger) 30%, transparent); }
.danger.small { padding: 5px 8px; }

.add-card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 16px 20px; margin-bottom: 16px; }
.add-card h3 { margin: 0 0 12px; font-size: 13px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; }
.form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-items: end; }
.form-grid label { display: flex; flex-direction: column; gap: 4px; }
.form-grid label span { font-size: 11px; color: var(--color-text-muted); font-weight: 500; }
.form-grid input, .form-grid select { padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px; }
.form-grid input:focus, .form-grid select:focus { outline: none; border-color: var(--color-accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent); }
.span-2 { grid-column: 1 / -1; }
.form-actions { grid-column: 1 / -1; display: flex; gap: 8px; justify-content: flex-end; }

.error { color: var(--color-danger); background: color-mix(in srgb, var(--color-danger) 12%, transparent); padding: 8px 12px; border-radius: 6px; font-size: 12px; margin-bottom: 12px; }
.table-wrap { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; overflow: hidden; }
table { width: 100%; border-collapse: collapse; }
thead th { text-align: left; padding: 8px 14px; font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--color-text-dim); background: color-mix(in srgb, var(--color-surface-hover) 50%, transparent); border-bottom: 1px solid var(--color-border); }
tbody td { padding: 8px 14px; font-size: 12px; border-top: 1px solid var(--color-border); }
tbody tr:first-child td { border-top: 0; }
.cell-name { display: flex; align-items: center; gap: 6px; color: var(--color-text-muted); }
.mono { font-family: var(--font-mono); color: var(--color-text); }
.muted { color: var(--color-text-muted); }
.small { font-size: 11px; }
.badge { display: inline-block; padding: 1px 6px; background: var(--color-surface-hover); border-radius: 4px; font-size: 11px; font-family: var(--font-mono); color: var(--color-text-muted); }
.toggle { display: inline-flex; align-items: center; gap: 4px; padding: 4px 8px; background: transparent; border: 1px solid var(--color-border); border-radius: 4px; color: var(--color-text-muted); font-size: 11px; }
.toggle.on { color: var(--color-success); border-color: color-mix(in srgb, var(--color-success) 30%, transparent); }
.toggle:hover { background: var(--color-surface-hover); }
.actions { text-align: right; }
.empty { text-align: center; color: var(--color-text-dim); padding: 24px; }
.loading { padding: 24px; color: var(--color-text-muted); text-align: center; }
</style>
