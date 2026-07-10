<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhKey, PhPlus, PhTrash, PhArrowRight, PhPencilSimple } from '@phosphor-icons/vue'
import { useProfilesStore } from '@/stores/profiles'
import { useRemotesStore } from '@/stores/remotes'
import type { Profile } from '@/api/types'
import { SYNC_ACTIONS } from '@/constants/forms'
import RemotePathField from '@/components/forms/RemotePathField.vue'
import { useConfirmDialog, useToast } from '@gnas/ui-shared'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'
import AppSectionLoading from '@gnas/ui-shared/components/AppSectionLoading.vue'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'

const store = useProfilesStore()
const remotes = useRemotesStore()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()

const formOpen = ref(false)
const formMode = ref<'create' | 'edit'>('create')
const draft = ref<Profile>(emptyDraft())

function emptyDraft(): Profile {
  return {
    name: '',
    from: '',
    to: '',
    direction: 'push',
    parallel: 4,
    bandwidth: 0,
    dry_run: false,
  }
}

onMounted(async () => {
  await Promise.all([store.load(), remotes.load()])
})

function openCreate() {
  formMode.value = 'create'
  draft.value = emptyDraft()
  formOpen.value = true
}

function openEdit(p: Profile) {
  formMode.value = 'edit'
  draft.value = {
    ...p,
    direction: p.direction || 'push',
    parallel: p.parallel || 4,
    bandwidth: p.bandwidth ?? 0,
    dry_run: !!p.dry_run,
  }
  formOpen.value = true
}

function closeForm() {
  formOpen.value = false
  draft.value = emptyDraft()
}

async function submitForm() {
  if (!draft.value.name || !draft.value.from || !draft.value.to) {
    toast.error('Name, From, To are required')
    return
  }
  try {
    if (formMode.value === 'create') {
      await store.add({ ...draft.value })
      toast.success('Profile added')
    } else {
      await store.update({ ...draft.value })
      toast.success('Profile updated')
    }
    closeForm()
  } catch {
    // api.error already set
  }
}

async function doDelete(name: string) {
  const ok = await confirmDialog({
    title: 'Delete profile',
    message: `Delete profile "${name}"?`,
    confirmText: 'Delete',
    confirmVariant: 'danger',
  })
  if (!ok) return
  await store.remove(name)
}
</script>

<template>
  <div class="profiles-page" data-testid="page-profiles">
    <header class="page-header">
      <div>
        <h1>Profiles</h1>
        <p class="sub">Sync profiles in <code>gn-drive.db</code>.</p>
      </div>
      <button class="primary" data-testid="profiles-add" @click="formOpen ? closeForm() : openCreate()">
        <PhPlus :size="16" weight="bold" /> {{ formOpen && formMode === 'create' ? 'Close' : 'Add profile' }}
      </button>
    </header>

    <div v-if="formOpen" class="add-card" :data-testid="formMode === 'create' ? 'profiles-add-form' : 'profiles-edit-form'">
      <h3>{{ formMode === 'create' ? 'New profile' : `Edit profile` }}</h3>
      <form @submit.prevent="submitForm" class="form-grid">
        <label>
          <span>Name</span>
          <input
            v-model="draft.name"
            required
            data-testid="profiles-name"
            :readonly="formMode === 'edit'"
            :class="{ readonly: formMode === 'edit' }"
          />
        </label>
        <label>
          <span>Direction</span>
          <select v-model="draft.direction" data-testid="profiles-direction">
            <option v-for="a in SYNC_ACTIONS" :key="a" :value="a">{{ a }}</option>
          </select>
        </label>

        <div class="span-2">
          <RemotePathField
            v-model="draft.from"
            :remotes="remotes.items"
            test-id="profiles-from"
            label="From (remote:path or /absolute)"
            required
          />
        </div>
        <div class="span-2">
          <RemotePathField
            v-model="draft.to"
            :remotes="remotes.items"
            test-id="profiles-to"
            label="To (remote:path or /absolute)"
            required
          />
        </div>

        <label><span>Parallel</span><input v-model.number="draft.parallel" type="number" min="1" max="64" data-testid="profiles-parallel" /></label>
        <label><span>Bandwidth (MB/s)</span><input v-model.number="draft.bandwidth" type="number" min="0" data-testid="profiles-bandwidth" /></label>
        <label class="span-2 checkbox">
          <input v-model="draft.dry_run" type="checkbox" data-testid="profiles-dry-run" /> Dry run (preview only)
        </label>
        <div class="form-actions">
          <button type="button" class="ghost" @click="closeForm">Cancel</button>
          <button type="submit" class="primary" :disabled="store.loading" data-testid="profiles-submit">
            {{ store.loading ? 'Saving…' : formMode === 'create' ? 'Add' : 'Save' }}
          </button>
        </div>
      </form>
    </div>

    <AppAlert v-if="store.error" type="error">{{ store.error }}</AppAlert>

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
            <td class="mono small" :title="p.from">{{ p.from }}</td>
            <td class="arrow"><PhArrowRight :size="12" weight="bold" /></td>
            <td class="mono small" :title="p.to">{{ p.to }}</td>
            <td><span class="badge">{{ p.direction || '-' }}</span></td>
            <td class="num">{{ p.parallel }}</td>
            <td class="num">{{ p.bandwidth > 0 ? p.bandwidth + 'M' : '∞' }}</td>
            <td class="actions">
              <button class="ghost small" :data-testid="`profiles-edit-${p.name}`" title="Edit" @click="openEdit(p)">
                <PhPencilSimple :size="14" weight="regular" />
              </button>
              <button class="danger small" :data-testid="`profiles-delete-${p.name}`" @click="doDelete(p.name)">
                <PhTrash :size="14" weight="regular" />
              </button>
            </td>
          </tr>
          <tr v-if="store.items.length === 0 && !store.loading">
            <td colspan="8" class="empty"><EmptyState title="No profiles configured" /></td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else><AppSectionLoading label="Loading profiles..." /></div>
  </div>
</template>

<style scoped>
.profiles-page { max-width: 1200px; margin: 0 auto; }
.page-header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px; gap: 16px; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.page-header code { font-family: var(--font-mono); font-size: 12px; padding: 1px 4px; background: var(--color-surface-hover); border-radius: 3px; }
.primary { display: inline-flex; align-items: center; gap: 6px; padding: 7px 14px; background: var(--color-accent); color: white; border: 0; border-radius: 6px; font-size: 13px; font-weight: 500; }
.ghost { display: inline-flex; align-items: center; padding: 6px 10px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-size: 12px; }
.ghost:hover { background: var(--color-surface-hover); }
.ghost.small { padding: 5px 8px; }
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
.form-grid input.readonly { opacity: 0.7; cursor: not-allowed; }
.span-2 { grid-column: 1 / -1; }
.checkbox { flex-direction: row !important; align-items: center; gap: 8px !important; }
.checkbox input { width: auto; }
.form-actions { grid-column: 1 / -1; display: flex; gap: 8px; justify-content: flex-end; }

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
.actions { text-align: right; white-space: nowrap; }
.actions button + button { margin-left: 4px; }
.empty { text-align: center; color: var(--color-text-dim); padding: 24px; }
</style>
