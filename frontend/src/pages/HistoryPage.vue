<script setup lang="ts">
import { onMounted, computed } from 'vue'
import { PhClockCounterClockwise, PhTrash } from '@phosphor-icons/vue'
import { useHistoryStore } from '@/stores/history'
import { useConfirmDialog } from '@gnas/ui-shared'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'

const store = useHistoryStore()
const { confirmDialog } = useConfirmDialog()
onMounted(() => store.load())

const totalBytes = computed(() => store.stats?.total_bytes ?? 0)
const totalDuration = computed(() => store.stats?.total_duration_secs ?? 0)

function formatBytes(n: number): string {
  const k = 1024
  if (n < k) return `${n} B`
  if (n < k * k) return `${(n / k).toFixed(1)} KB`
  if (n < k * k * k) return `${(n / (k * k)).toFixed(1)} MB`
  return `${(n / (k * k * k)).toFixed(2)} GB`
}

function formatDuration(s: number): string {
  if (s < 60) return `${s}s`
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`
  return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`
}

function stateColor(s: string): string {
  if (s === 'completed') return 'ok'
  if (s === 'failed') return 'fail'
  if (s === 'cancelled') return 'warn'
  return ''
}

async function doClear() {
  const ok = await confirmDialog({ title: 'Clear history', message: 'Clear all history? This cannot be undone.', confirmText: 'Clear', confirmVariant: 'danger' })
  if (!ok) return
  await store.clear()
}
</script>

<template>
  <div class="history-page">
    <header class="page-header">
      <div>
        <h1>History</h1>
        <p class="sub">Past sync runs (capped at 1000 rows).</p>
      </div>
      <button class="danger" @click="doClear"><PhTrash :size="14" weight="regular" /> Clear all</button>
    </header>

    <div class="stats-grid">
      <div class="stat-card">
        <div class="label">Total syncs</div>
        <div class="value">{{ store.stats?.total_syncs ?? 0 }}</div>
      </div>
      <div class="stat-card">
        <div class="label">Total bytes</div>
        <div class="value">{{ formatBytes(totalBytes) }}</div>
      </div>
      <div class="stat-card">
        <div class="label">Total errors</div>
        <div class="value" :class="{ danger: (store.stats?.total_errors ?? 0) > 0 }">
          {{ store.stats?.total_errors ?? 0 }}
        </div>
      </div>
      <div class="stat-card">
        <div class="label">Total duration</div>
        <div class="value">{{ formatDuration(totalDuration) }}</div>
      </div>
    </div>

    <div v-if="store.stats?.by_profile && Object.keys(store.stats.by_profile).length > 0" class="by-profile">
      <h3>By profile</h3>
      <div class="by-grid">
        <div v-for="(s, name) in store.stats.by_profile" :key="name" class="by-card">
          <div class="by-name mono">{{ name }}</div>
          <div class="by-row"><span>Syncs</span><span class="mono">{{ s.syncs }}</span></div>
          <div class="by-row"><span>Bytes</span><span class="mono">{{ formatBytes(s.bytes) }}</span></div>
          <div class="by-row"><span>Errors</span><span class="mono">{{ s.errors }}</span></div>
          <div class="by-row"><span>Duration</span><span class="mono">{{ formatDuration(s.duration_secs) }}</span></div>
        </div>
      </div>
    </div>

    <div class="table-wrap">
      <table>
        <thead>
          <tr><th>Profile</th><th>Action</th><th>State</th><th>Bytes</th><th>Files</th><th>Errors</th><th>Duration</th><th>Started</th></tr>
        </thead>
        <tbody>
          <tr v-for="e in store.entries" :key="e.id">
            <td class="mono small">{{ e.profile_name }}</td>
            <td><span class="badge">{{ e.action }}</span></td>
            <td><span class="badge" :class="stateColor(e.state)">{{ e.state }}</span></td>
            <td class="num">{{ formatBytes(e.bytes) }}</td>
            <td class="num">{{ e.files }}</td>
            <td class="num" :class="{ danger: e.errors > 0 }">{{ e.errors }}</td>
            <td class="num">{{ formatDuration(e.duration_secs) }}</td>
            <td class="muted small">{{ e.started_at }}</td>
          </tr>
          <tr v-if="store.entries.length === 0 && !store.loading">
            <td colspan="8" class="empty">
              <EmptyState title="No history yet">
                <template #icon>
                  <PhClockCounterClockwise :size="32" weight="light" />
                </template>
              </EmptyState>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.history-page { max-width: 1200px; margin: 0 auto; }
.page-header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px; gap: 16px; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.danger { display: inline-flex; align-items: center; gap: 4px; padding: 6px 10px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-muted); font-size: 12px; }
.danger:hover { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); border-color: color-mix(in srgb, var(--color-danger) 30%, transparent); }

.stats-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 10px; margin-bottom: 20px; }
.stat-card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 12px 14px; }
.stat-card .label { font-size: 11px; color: var(--color-text-dim); text-transform: uppercase; letter-spacing: 0.4px; margin-bottom: 6px; }
.stat-card .value { font-size: 20px; font-weight: 700; font-family: var(--font-mono); font-variant-numeric: tabular-nums; line-height: 1; }
.value.danger { color: var(--color-danger); }

.by-profile { margin-bottom: 20px; }
.by-profile h3 { font-size: 12px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; margin: 0 0 8px; }
.by-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 8px; }
.by-card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 6px; padding: 10px 12px; }
.by-name { font-size: 13px; font-weight: 600; margin-bottom: 6px; }
.by-row { display: flex; justify-content: space-between; font-size: 11px; padding: 2px 0; color: var(--color-text-muted); }
.by-row .mono { color: var(--color-text); }

.table-wrap { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; overflow: hidden; }
table { width: 100%; border-collapse: collapse; }
thead th { text-align: left; padding: 8px 14px; font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--color-text-dim); background: color-mix(in srgb, var(--color-surface-hover) 50%, transparent); border-bottom: 1px solid var(--color-border); }
tbody td { padding: 8px 14px; font-size: 12px; border-top: 1px solid var(--color-border); }
tbody tr:first-child td { border-top: 0; }
.mono { font-family: var(--font-mono); }
.muted { color: var(--color-text-muted); }
.small { font-size: 11px; }
.badge { display: inline-block; padding: 1px 6px; background: var(--color-surface-hover); border-radius: 4px; font-size: 10px; font-family: var(--font-mono); color: var(--color-text-muted); }
.badge.ok { color: var(--color-success); }
.badge.fail { color: var(--color-danger); }
.badge.warn { color: var(--color-warning); }
.num { font-family: var(--font-mono); text-align: right; }
.num.danger { color: var(--color-danger); }
.empty { text-align: center; color: var(--color-text-dim); padding: 40px; }
.empty p { margin-top: 8px; font-size: 13px; }
</style>
