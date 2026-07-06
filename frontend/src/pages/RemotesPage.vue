<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhCloud, PhPlus, PhTrash, PhCheckCircle, PhXCircle, PhSpinner } from '@phosphor-icons/vue'
import { useRemotesStore } from '@/stores/remotes'
import { useApi } from '@/composables/useApi'
import { useConfirmDialog } from '@gnas/ui-shared'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'
import AppSectionLoading from '@gnas/ui-shared/components/AppSectionLoading.vue'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'

const store = useRemotesStore()
const api = useApi()
const { confirmDialog } = useConfirmDialog()

const showAdd = ref(false)
const newName = ref('')
const newType = ref('local')
const testResults = ref<Record<string, { ok: boolean; error?: string }>>({})

onMounted(() => store.load())

async function submitAdd() {
  if (!newName.value || !newType.value) return
  try {
    await store.add(newName.value.trim(), newType.value.trim())
    showAdd.value = false
    newName.value = ''
    newType.value = 'local'
  } catch (e) {
    // api.error already set
  }
}

async function doTest(name: string) {
  testResults.value[name] = { ok: false } // pending
  const r = await store.test(name)
  testResults.value[name] = r
}

async function doDelete(name: string) {
  const ok = await confirmDialog({ title: 'Delete remote', message: `Delete remote "${name}"?`, confirmText: 'Delete', confirmVariant: 'danger' })
  if (!ok) return
  await store.remove(name)
}
</script>

<template>
  <div class="remotes-page">
    <header class="page-header">
      <div>
        <h1>Remotes</h1>
        <p class="sub">rclone remotes in <code>rclone.conf</code>.</p>
      </div>
      <button class="primary" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> Add remote
      </button>
    </header>

    <div v-if="showAdd" class="add-card">
      <h3>New remote</h3>
      <form @submit.prevent="submitAdd" class="add-form">
        <label>
          <span>Name</span>
          <input v-model="newName" placeholder="gdrive" required />
        </label>
        <label>
          <span>Type</span>
          <input v-model="newType" placeholder="drive, s3, local, sftp..." required />
        </label>
        <div class="add-actions">
          <button type="button" class="ghost" @click="showAdd = false">Cancel</button>
          <button type="submit" class="primary" :disabled="api.loading.value">
            {{ api.loading.value ? 'Adding…' : 'Add' }}
          </button>
        </div>
      </form>
      <p class="hint">
        For OAuth-based providers (drive, onedrive, etc.), add non-interactively
        and configure tokens via the desktop app — web UI only supports local
        filesystem and pre-configured remotes.
      </p>
    </div>

    <AppAlert v-if="api.error.value" type="error">{{ api.error.value }}</AppAlert>

    <div class="table-wrap" v-if="store.items.length > 0 || !store.loading">
      <table>
        <thead>
          <tr><th>Name</th><th>Type</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="r in store.items" :key="r.name">
            <td>
              <div class="cell-name">
                <PhCloud :size="16" weight="regular" />
                <span class="mono">{{ r.name }}</span>
              </div>
            </td>
            <td><span class="badge">{{ r.type || 'unknown' }}</span></td>
            <td class="actions">
              <button class="ghost small" @click="doTest(r.name)" :title="`Test ${r.name}`">
                <template v-if="testResults[r.name]?.ok === true">
                  <PhCheckCircle :size="16" weight="fill" class="ok" />
                </template>
                <template v-else-if="testResults[r.name]?.ok === false && testResults[r.name]?.error">
                  <PhXCircle :size="16" weight="fill" class="fail" />
                </template>
                <template v-else-if="store.loading">
                  <PhSpinner :size="16" class="spin" />
                </template>
                <template v-else>
                  Test
                </template>
              </button>
              <button class="danger small" @click="doDelete(r.name)" :title="`Delete ${r.name}`">
                <PhTrash :size="14" weight="regular" />
              </button>
            </td>
          </tr>
          <tr v-if="store.items.length === 0 && !store.loading">
            <td colspan="3" class="empty">
              <EmptyState title="No remotes configured" description="Use &quot;Add remote&quot; to create one." />
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else><AppSectionLoading /></div>
  </div>
</template>

<style scoped>
.remotes-page { max-width: 1100px; margin: 0 auto; }
.page-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
  margin-bottom: 20px;
  gap: 16px;
}
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.page-header code {
  font-family: var(--font-mono);
  font-size: 12px;
  padding: 1px 4px;
  background: var(--color-surface-hover);
  border-radius: 3px;
}
.primary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 7px 14px;
  background: var(--color-accent);
  color: white;
  border: 0;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 500;
}
.ghost {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 6px 10px;
  background: transparent;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-text);
  font-size: 12px;
}
.ghost:hover { background: var(--color-surface-hover); }
.ghost.small { padding: 5px 8px; }
.danger {
  display: inline-flex;
  align-items: center;
  padding: 5px 8px;
  background: transparent;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-text-muted);
  font-size: 12px;
}
.danger:hover {
  background: color-mix(in srgb, var(--color-danger) 12%, transparent);
  color: var(--color-danger);
  border-color: color-mix(in srgb, var(--color-danger) 30%, transparent);
}

.add-card {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  padding: 16px 20px;
  margin-bottom: 16px;
}
.add-card h3 { margin: 0 0 12px; font-size: 13px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; }
.add-form {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  align-items: end;
}
.add-form label { display: flex; flex-direction: column; gap: 4px; }
.add-form label span { font-size: 11px; color: var(--color-text-muted); font-weight: 500; }
.add-form input {
  padding: 7px 10px;
  background: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-text);
  font-family: var(--font-mono);
  font-size: 13px;
}
.add-form input:focus {
  outline: none;
  border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent);
}
.add-actions { grid-column: 1 / -1; display: flex; gap: 8px; justify-content: flex-end; }
.hint {
  margin: 12px 0 0;
  padding-top: 12px;
  border-top: 1px solid var(--color-border);
  font-size: 11px;
  color: var(--color-text-dim);
}

.error {
  color: var(--color-danger);
  background: color-mix(in srgb, var(--color-danger) 12%, transparent);
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 12px;
  margin-bottom: 12px;
}

.table-wrap {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  overflow: hidden;
}
table { width: 100%; border-collapse: collapse; }
thead th {
  text-align: left;
  padding: 8px 14px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  color: var(--color-text-dim);
  background: color-mix(in srgb, var(--color-surface-hover) 50%, transparent);
  border-bottom: 1px solid var(--color-border);
}
tbody td {
  padding: 8px 14px;
  font-size: 13px;
  border-top: 1px solid var(--color-border);
}
tbody tr:first-child td { border-top: 0; }
.cell-name { display: flex; align-items: center; gap: 8px; color: var(--color-text-muted); }
.mono { font-family: var(--font-mono); color: var(--color-text); }
.badge {
  display: inline-block;
  padding: 1px 8px;
  background: var(--color-surface-hover);
  border-radius: 4px;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--color-text-muted);
}
.actions { text-align: right; white-space: nowrap; }
.actions button + button { margin-left: 4px; }
.empty { text-align: center; color: var(--color-text-dim); padding: 24px; }
.loading { padding: 24px; color: var(--color-text-muted); text-align: center; }
.ok { color: var(--color-success); }
.fail { color: var(--color-danger); }
.spin { animation: spin 1s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
