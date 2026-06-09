<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhKey, PhPlus, PhTrash, PhArrowRight } from '@phosphor-icons/vue'
import { useProfilesStore } from '@/stores/profiles'
import { useApi } from '@/composables/useApi'
import type { Profile } from '@/api/types'

const store = useProfilesStore()
const api = useApi()

const showAdd = ref(false)
const draft = ref<Profile>({
  name: '',
  from: '',
  to: '',
  parallel: 4,
  bandwidth: 0,
  dry_run: false,
})

onMounted(() => store.load())

async function submitAdd() {
  if (!draft.value.name || !draft.value.from || !draft.value.to) {
    alert('Name, From, To are required')
    return
  }
  await store.add({ ...draft.value })
  showAdd.value = false
  draft.value = { name: '', from: '', to: '', parallel: 4, bandwidth: 0, dry_run: false }
}

async function doDelete(name: string) {
  if (!confirm(`Delete profile "${name}"?`)) return
  await store.remove(name)
}
</script>

<template>
  <div class="profiles-page">
    <header class="page-header">
      <div>
        <h1>Profiles</h1>
        <p class="sub">Sync profiles in <code>gn-drive.db</code>.</p>
      </div>
      <button class="primary" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> Add profile
      </button>
    </header>

    <div v-if="showAdd" class="add-card">
      <h3>New profile</h3>
      <form @submit.prevent="submitAdd" class="form-grid">
        <label><span>Name</span><input v-model="draft.name" required /></label>
        <label><span>Direction</span>
          <select v-model="draft.direction">
            <option value="">—</option>
            <option value="pull">pull</option>
            <option value="push">push</option>
            <option value="bi">bi</option>
            <option value="bi-resync">bi-resync</option>
          </select>
        </label>
        <label class="span-2"><span>From (remote:path)</span><input v-model="draft.from" placeholder="gdrive:/Backup" required /></label>
        <label class="span-2"><span>To (remote:path)</span><input v-model="draft.to" placeholder="local:/data/Backup" required /></label>
        <label><span>Parallel</span><input v-model.number="draft.parallel" type="number" min="1" max="64" /></label>
        <label><span>Bandwidth (MB/s)</span><input v-model.number="draft.bandwidth" type="number" min="0" /></label>
        <label class="span-2 checkbox">
          <input v-model="draft.dry_run" type="checkbox" /> Dry run (preview only)
        </label>
        <div class="form-actions">
          <button type="button" class="ghost" @click="showAdd = false">Cancel</button>
          <button type="submit" class="primary" :disabled="api.loading.value">
            {{ api.loading.value ? 'Adding…' : 'Add' }}
          </button>
        </div>
      </form>
    </div>

    <div v-if="api.error.value" class="error">{{ api.error.value }}</div>

    <div class="table-wrap" v-if="store.items.length > 0 || !store.loading">
      <table>
        <thead>
          <tr><th>Name</th><th>From</th><th></th><th>To</th><th>Dir</th><th>Par</th><th>BW</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="p in store.items" :key="p.name">
            <td>
              <div class="cell-name">
                <PhKey :size="14" weight="regular" />
                <span class="mono">{{ p.name }}</span>
              </div>
            </td>
            <td class="mono small">{{ p.from }}</td>
            <td class="arrow"><PhArrowRight :size="12" weight="bold" /></td>
            <td class="mono small">{{ p.to }}</td>
            <td><span class="badge">{{ p.direction || '—' }}</span></td>
            <td class="num">{{ p.parallel }}</td>
            <td class="num">{{ p.bandwidth > 0 ? p.bandwidth + 'M' : '∞' }}</td>
            <td class="actions">
              <button class="danger small" @click="doDelete(p.name)">
                <PhTrash :size="14" weight="regular" />
              </button>
            </td>
          </tr>
          <tr v-if="store.items.length === 0 && !store.loading">
            <td colspan="8" class="empty">No profiles configured.</td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else class="loading">Loading…</div>
  </div>
</template>

<style scoped>
.profiles-page { max-width: 1200px; margin: 0 auto; }
.page-header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px; gap: 16px; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.page-header code { font-family: var(--font-mono); font-size: 12px; padding: 1px 4px; background: var(--color-surface-hover); border-radius: 3px; }
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
.form-grid input, .form-grid select {
  padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 6px;
  color: var(--color-text); font-family: var(--font-mono); font-size: 13px;
}
.form-grid input:focus, .form-grid select:focus {
  outline: none; border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent);
}
.span-2 { grid-column: 1 / -1; }
.checkbox { flex-direction: row !important; align-items: center; gap: 8px !important; }
.checkbox input { width: auto; }
.form-actions { grid-column: 1 / -1; display: flex; gap: 8px; justify-content: flex-end; }

.error { color: var(--color-danger); background: color-mix(in srgb, var(--color-danger) 12%, transparent); padding: 8px 12px; border-radius: 6px; font-size: 12px; margin-bottom: 12px; }

.table-wrap { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; overflow: hidden; }
table { width: 100%; border-collapse: collapse; }
thead th { text-align: left; padding: 8px 12px; font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--color-text-dim); background: color-mix(in srgb, var(--color-surface-hover) 50%, transparent); border-bottom: 1px solid var(--color-border); }
tbody td { padding: 8px 12px; font-size: 12px; border-top: 1px solid var(--color-border); }
tbody tr:first-child td { border-top: 0; }
.cell-name { display: flex; align-items: center; gap: 6px; color: var(--color-text-muted); }
.mono { font-family: var(--font-mono); color: var(--color-text); }
.mono.small { font-size: 11px; max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.badge { display: inline-block; padding: 1px 6px; background: var(--color-surface-hover); border-radius: 4px; font-size: 11px; font-family: var(--font-mono); color: var(--color-text-muted); }
.num { font-family: var(--font-mono); color: var(--color-text-muted); text-align: right; }
.arrow { color: var(--color-text-dim); text-align: center; }
.actions { text-align: right; }
.empty { text-align: center; color: var(--color-text-dim); padding: 24px; }
.loading { padding: 24px; color: var(--color-text-muted); text-align: center; }
</style>
