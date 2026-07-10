<script setup lang="ts">
import { onMounted, computed, ref } from 'vue'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { PhClockCounterClockwise, PhTrash } from '@phosphor-icons/vue'
import { useHistoryStore } from '@/stores/history'
import { useConfirmDialog } from '@gnas/ui-shared'
import EmptyState from '@gnas/ui-shared/components/EmptyState.vue'

const store = useHistoryStore()
const { confirmDialog } = useConfirmDialog()
// History is capped at 1000 rows server-side; fetch the full cap and let the
// virtualizer below render only the rows in view instead of paginating.
onMounted(() => store.load(1000))

const columnsTemplate = '1.2fr 0.8fr 0.8fr 0.8fr 0.6fr 0.6fr 0.8fr 1.2fr'

const scrollParentEl = ref<HTMLElement | null>(null)
const rowVirtualizer = useVirtualizer(computed(() => ({
  count: store.entries.length,
  getScrollElement: () => scrollParentEl.value,
  estimateSize: () => 33,
  overscan: 12,
})))
const virtualRows = computed(() => rowVirtualizer.value.getVirtualItems())
const totalSize = computed(() => rowVirtualizer.value.getTotalSize())

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
  <div class="history-page" data-testid="page-history">
    <header class="page-header">
      <div>
        <h1>History</h1>
        <p class="sub">Past sync runs (capped at 1000 rows).</p>
      </div>
      <button class="danger" data-testid="history-clear" @click="doClear"><PhTrash :size="14" weight="regular" /> Clear all</button>
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
      <div class="row head-row" :style="{ gridTemplateColumns: columnsTemplate }">
        <div>Profile</div><div>Action</div><div>State</div><div>Bytes</div>
        <div>Files</div><div>Errors</div><div>Duration</div><div>Started</div>
      </div>

      <EmptyState v-if="store.entries.length === 0 && !store.loading" title="No history yet" class="empty">
        <template #icon>
          <PhClockCounterClockwise :size="32" weight="light" />
        </template>
      </EmptyState>

      <!--
        Virtualized: History caps at 1000 rows, so only the ~20 rows in the
        viewport (+ overscan) are ever mounted, regardless of total count.
      -->
      <div v-else ref="scrollParentEl" class="tbody-scroll">
        <div class="virtual-spacer" :style="{ height: `${totalSize}px` }">
          <div
            v-for="vRow in virtualRows"
            :key="vRow.index"
            class="row body-row"
            :style="{ gridTemplateColumns: columnsTemplate, transform: `translateY(${vRow.start}px)` }"
          >
            <div class="mono small">{{ store.entries[vRow.index].profile_name }}</div>
            <div><span class="badge">{{ store.entries[vRow.index].action }}</span></div>
            <div><span class="badge" :class="stateColor(store.entries[vRow.index].state)">{{ store.entries[vRow.index].state }}</span></div>
            <div class="num">{{ formatBytes(store.entries[vRow.index].bytes) }}</div>
            <div class="num">{{ store.entries[vRow.index].files }}</div>
            <div class="num" :class="{ danger: store.entries[vRow.index].errors > 0 }">{{ store.entries[vRow.index].errors }}</div>
            <div class="num">{{ formatDuration(store.entries[vRow.index].duration_secs) }}</div>
            <div class="muted small">{{ store.entries[vRow.index].started_at }}</div>
          </div>
        </div>
      </div>
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

/* Grid-based rows (not a native <table>) so the virtualizer can absolutely
   position only the rows in view; head and body share one column template
   to stay aligned. */
.row { display: grid; align-items: center; gap: 8px; padding: 0 14px; }
.head-row { height: 33px; font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--color-text-dim); background: color-mix(in srgb, var(--color-surface-hover) 50%, transparent); border-bottom: 1px solid var(--color-border); }
.tbody-scroll { max-height: 60vh; overflow-y: auto; }
.virtual-spacer { position: relative; width: 100%; }
.body-row { position: absolute; top: 0; left: 0; width: 100%; height: 33px; font-size: 12px; border-top: 1px solid var(--color-border); }
.body-row:first-child { border-top: 0; }
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
