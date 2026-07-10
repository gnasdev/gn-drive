<script setup lang="ts">
import { onMounted, computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useVirtualizer } from '@tanstack/vue-virtual'
import { PhClockCounterClockwise, PhTrash } from '@phosphor-icons/vue'
import { useHistoryStore } from '@/stores/history'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import EmptyState from '@/components/ui/EmptyState.vue'
import { cn } from '@/lib/cn'

const { t } = useI18n()
const store = useHistoryStore()
const { confirmDialog } = useConfirmDialog()
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
  if (s === 'completed') return 'text-success'
  if (s === 'failed') return 'text-danger'
  if (s === 'cancelled') return 'text-warning'
  return ''
}

async function doClear() {
  const ok = await confirmDialog({
    title: t('history.clearTitle'),
    message: t('history.clearMessage'),
    confirmText: t('common.clear'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await store.clear()
}
</script>

<template>
  <div class="page-shell-wide" data-testid="page-history">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('history.title') }}</h1>
        <p class="page-sub">{{ t('history.sub') }}</p>
      </div>
      <button class="danger" data-testid="history-clear" @click="doClear">
        <PhTrash :size="14" weight="regular" /> {{ t('history.clearAll') }}
      </button>
    </header>

    <div class="mb-5 grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-2.5">
      <div class="card px-3.5 py-3">
        <div class="mb-1.5 text-[11px] uppercase tracking-wide text-text-dim">{{ t('history.totalSyncs') }}</div>
        <div class="font-mono text-xl font-bold tabular-nums leading-none">{{ store.stats?.total_syncs ?? 0 }}</div>
      </div>
      <div class="card px-3.5 py-3">
        <div class="mb-1.5 text-[11px] uppercase tracking-wide text-text-dim">{{ t('history.totalBytes') }}</div>
        <div class="font-mono text-xl font-bold tabular-nums leading-none">{{ formatBytes(totalBytes) }}</div>
      </div>
      <div class="card px-3.5 py-3">
        <div class="mb-1.5 text-[11px] uppercase tracking-wide text-text-dim">{{ t('history.totalErrors') }}</div>
        <div
          :class="cn(
            'font-mono text-xl font-bold tabular-nums leading-none',
            (store.stats?.total_errors ?? 0) > 0 && 'text-danger',
          )"
        >
          {{ store.stats?.total_errors ?? 0 }}
        </div>
      </div>
      <div class="card px-3.5 py-3">
        <div class="mb-1.5 text-[11px] uppercase tracking-wide text-text-dim">{{ t('history.totalDuration') }}</div>
        <div class="font-mono text-xl font-bold tabular-nums leading-none">{{ formatDuration(totalDuration) }}</div>
      </div>
    </div>

    <div v-if="store.stats?.by_profile && Object.keys(store.stats.by_profile).length > 0" class="mb-5">
      <h3 class="section-label">{{ t('history.byProfile') }}</h3>
      <div class="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-2">
        <div v-for="(s, name) in store.stats.by_profile" :key="name" class="card px-3 py-2.5">
          <div class="mb-1.5 font-mono text-[13px] font-semibold">{{ name }}</div>
          <div class="flex justify-between py-0.5 text-[11px] text-text-muted">
            <span>{{ t('history.syncs') }}</span><span class="font-mono text-text">{{ s.syncs }}</span>
          </div>
          <div class="flex justify-between py-0.5 text-[11px] text-text-muted">
            <span>{{ t('history.bytes') }}</span><span class="font-mono text-text">{{ formatBytes(s.bytes) }}</span>
          </div>
          <div class="flex justify-between py-0.5 text-[11px] text-text-muted">
            <span>{{ t('history.errors') }}</span><span class="font-mono text-text">{{ s.errors }}</span>
          </div>
          <div class="flex justify-between py-0.5 text-[11px] text-text-muted">
            <span>{{ t('history.duration') }}</span><span class="font-mono text-text">{{ formatDuration(s.duration_secs) }}</span>
          </div>
        </div>
      </div>
    </div>

    <div class="table-wrap">
      <div
        class="grid h-[33px] items-center gap-2 border-b border-border bg-surface-hover/50 px-3.5 text-[10px] font-semibold uppercase tracking-wide text-text-dim"
        :style="{ gridTemplateColumns: columnsTemplate }"
      >
        <div>{{ t('history.colProfile') }}</div>
        <div>{{ t('history.colAction') }}</div>
        <div>{{ t('history.colState') }}</div>
        <div>{{ t('history.colBytes') }}</div>
        <div>{{ t('history.colFiles') }}</div>
        <div>{{ t('history.colErrors') }}</div>
        <div>{{ t('history.colDuration') }}</div>
        <div>{{ t('history.colStarted') }}</div>
      </div>

      <EmptyState v-if="store.entries.length === 0 && !store.loading" :title="t('history.empty')">
        <template #icon>
          <PhClockCounterClockwise :size="32" weight="light" />
        </template>
      </EmptyState>

      <div v-else ref="scrollParentEl" class="max-h-[60vh] overflow-y-auto">
        <div class="relative w-full" :style="{ height: `${totalSize}px` }">
          <div
            v-for="vRow in virtualRows"
            :key="vRow.index"
            class="absolute top-0 left-0 grid h-[33px] w-full items-center gap-2 border-t border-border px-3.5 text-xs"
            :style="{ gridTemplateColumns: columnsTemplate, transform: `translateY(${vRow.start}px)` }"
          >
            <div class="font-mono text-[11px]">{{ store.entries[vRow.index].profile_name }}</div>
            <div><span class="badge !text-[10px]">{{ store.entries[vRow.index].action }}</span></div>
            <div>
              <span :class="cn('badge !text-[10px]', stateColor(store.entries[vRow.index].state))">
                {{ store.entries[vRow.index].state }}
              </span>
            </div>
            <div class="text-right font-mono">{{ formatBytes(store.entries[vRow.index].bytes) }}</div>
            <div class="text-right font-mono">{{ store.entries[vRow.index].files }}</div>
            <div
              :class="cn(
                'text-right font-mono',
                store.entries[vRow.index].errors > 0 && 'text-danger',
              )"
            >
              {{ store.entries[vRow.index].errors }}
            </div>
            <div class="text-right font-mono">{{ formatDuration(store.entries[vRow.index].duration_secs) }}</div>
            <div class="text-[11px] text-text-muted">{{ store.entries[vRow.index].started_at }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
