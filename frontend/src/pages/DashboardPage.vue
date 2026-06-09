<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { api } from '@/api/client'
import {
  PhKey,
  PhCloud,
  PhClockCounterClockwise,
  PhCircleNotch,
  PhStack,
} from '@phosphor-icons/vue'

interface Profile { name: string; direction: string }
interface Remote { name: string; type: string }
interface Task { id: string; name: string; action: string; status: string }
interface HistoryStats {
  total_syncs: number
  total_bytes: number
  total_errors: number
}

const profiles = ref<Profile[]>([])
const remotes = ref<Remote[]>([])
const tasks = ref<Task[]>([])
const stats = ref<HistoryStats | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)

onMounted(async () => {
  try {
    const [p, r, t, h] = await Promise.all([
      api.get<Profile[]>('/api/v1/profiles'),
      api.get<Remote[]>('/api/v1/remotes'),
      api.get<Task[]>('/api/v1/sync/tasks'),
      api.get<HistoryStats>('/api/v1/history/stats'),
    ])
    profiles.value = p ?? []
    remotes.value = r ?? []
    tasks.value = t ?? []
    stats.value = h ?? null
  } catch (e: any) {
    error.value = e?.message ?? 'failed to load dashboard'
  } finally {
    loading.value = false
  }
})

const activeTasks = computed(() => tasks.value.filter((t) => t.status === 'running'))
</script>

<template>
  <div class="dashboard">
    <header class="page-header">
      <h1>Dashboard</h1>
      <p class="sub">Overview of profiles, remotes, and recent sync activity.</p>
    </header>

    <div v-if="error" class="error">{{ error }}</div>
    <div v-if="loading" class="loading">Loading…</div>

    <div v-else class="grid">
      <div class="card stat">
        <div class="stat-head">
          <PhKey :size="18" weight="regular" />
          <span class="label">Profiles</span>
        </div>
        <div class="value">{{ profiles.length }}</div>
        <div class="foot">
          {{ profiles.filter(p => p.direction === 'bi' || p.direction === 'bi-resync').length }} bi-directional
        </div>
      </div>

      <div class="card stat">
        <div class="stat-head">
          <PhCloud :size="18" weight="regular" />
          <span class="label">Remotes</span>
        </div>
        <div class="value">{{ remotes.length }}</div>
        <div class="foot">
          {{ new Set(remotes.map(r => r.type)).size }} unique providers
        </div>
      </div>

      <div class="card stat">
        <div class="stat-head">
          <PhCircleNotch :size="18" weight="regular" />
          <span class="label">Active tasks</span>
        </div>
        <div class="value">{{ activeTasks.length }}</div>
        <div class="foot" v-if="activeTasks.length > 0">
          {{ activeTasks[0].name }} ({{ activeTasks[0].action }})
        </div>
        <div class="foot" v-else>idle</div>
      </div>

      <div class="card stat">
        <div class="stat-head">
          <PhStack :size="18" weight="regular" />
          <span class="label">Total syncs</span>
        </div>
        <div class="value">{{ stats?.total_syncs ?? 0 }}</div>
        <div class="foot">
          {{ humanBytes(stats?.total_bytes ?? 0) }} transferred
        </div>
      </div>
    </div>

    <section class="section">
      <h2>Recent activity</h2>
      <div v-if="stats?.total_syncs === 0" class="empty">
        No syncs yet. Create a profile and a remote, then trigger your first sync.
      </div>
      <div v-else class="muted">Run history is shown on the History page.</div>
    </section>

    <section class="section">
      <h2>Quick links</h2>
      <div class="quick">
        <RouterLink :to="{ name: 'profiles' }" class="quick-link">
          <PhKey :size="16" weight="regular" />
          <span>Manage profiles</span>
        </RouterLink>
        <RouterLink :to="{ name: 'remotes' }" class="quick-link">
          <PhCloud :size="16" weight="regular" />
          <span>Configure remotes</span>
        </RouterLink>
        <RouterLink :to="{ name: 'history' }" class="quick-link">
          <PhClockCounterClockwise :size="16" weight="regular" />
          <span>View history</span>
        </RouterLink>
      </div>
    </section>
  </div>
</template>

<script lang="ts">
function humanBytes(n: number): string {
  const k = 1024
  if (n < k) return `${n} B`
  if (n < k * k) return `${(n / k).toFixed(1)} KB`
  if (n < k * k * k) return `${(n / (k * k)).toFixed(1)} MB`
  return `${(n / (k * k * k)).toFixed(2)} GB`
}
</script>

<style scoped>
.dashboard {
  max-width: 1100px;
  margin: 0 auto;
}
.page-header { margin-bottom: 24px; }
.page-header h1 {
  font-size: 22px;
  font-weight: 600;
  margin: 0 0 4px;
}
.sub {
  color: var(--color-text-muted);
  font-size: 13px;
  margin: 0;
}
.error {
  color: var(--color-danger);
  background: color-mix(in srgb, var(--color-danger) 12%, transparent);
  padding: 10px 14px;
  border-radius: 6px;
  font-size: 13px;
  margin-bottom: 16px;
}
.loading {
  color: var(--color-text-muted);
  font-size: 13px;
  padding: 20px 0;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
  margin-bottom: 32px;
}

.card {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  padding: 16px;
}
.stat-head {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--color-text-muted);
  font-size: 12px;
  font-weight: 500;
  margin-bottom: 8px;
}
.label { text-transform: uppercase; letter-spacing: 0.4px; font-size: 11px; }
.value {
  font-size: 28px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  font-family: var(--font-mono);
  line-height: 1;
}
.foot {
  font-size: 11px;
  color: var(--color-text-dim);
  margin-top: 6px;
  font-family: var(--font-mono);
}

.section {
  margin-top: 24px;
}
.section h2 {
  font-size: 13px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--color-text-muted);
  margin: 0 0 12px;
}
.empty {
  background: var(--color-surface);
  border: 1px dashed var(--color-border);
  border-radius: 8px;
  padding: 20px;
  text-align: center;
  color: var(--color-text-muted);
  font-size: 13px;
}
.muted { color: var(--color-text-dim); font-size: 12px; }

.quick {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.quick-link {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-text);
  font-size: 12px;
  transition: background-color 0.1s;
}
.quick-link:hover { background: var(--color-surface-hover); }
</style>
