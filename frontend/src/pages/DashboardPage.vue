<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { api } from '@/api/client'
import {
  PhKey,
  PhCloud,
  PhCircleNotch,
  PhStack,
} from '@phosphor-icons/vue'
import SkeletonCard from '@/components/ui/SkeletonCard.vue'
import EmptyState from '@/components/ui/EmptyState.vue'
import { useSwrCache } from '@/composables/useSwrCache'

interface Profile { name: string; direction: string }
interface Remote { name: string; type: string }
interface Task { id: string; name: string; action: string; status: string }
interface DashboardOverview {
  profiles: Profile[]
  remotes: Remote[]
  tasks: Task[]
}

const { t } = useI18n()

const { data, state: cacheState, error } = useSwrCache<DashboardOverview>({
  namespace: 'gn-drive',
  key: 'dashboard:overview',
  userScope: () => 'local',
  ttlMs: 30_000,
  fetcher: async () => {
    const [p, r, tsk] = await Promise.all([
      api.get<Profile[]>('/api/v1/profiles'),
      api.get<Remote[]>('/api/v1/remotes'),
      api.get<Task[]>('/api/v1/sync/tasks'),
    ])
    return { profiles: p ?? [], remotes: r ?? [], tasks: tsk ?? [] }
  },
})

const profiles = computed(() => data.value?.profiles ?? [])
const remotes = computed(() => data.value?.remotes ?? [])
const tasks = computed(() => data.value?.tasks ?? [])
const activeTasks = computed(() => tasks.value.filter((x) => x.status === 'running'))
const loadErrorMessage = computed(() => {
  if (!error.value || data.value) return null
  return (error.value as { message?: string })?.message ?? t('dashboard.loadFailed')
})
const showSkeleton = computed(() => cacheState.value === 'hydrating' && !data.value)
</script>

<template>
  <div class="page-shell" data-testid="page-dashboard">
    <header class="mb-6">
      <h1 class="page-title">{{ t('dashboard.title') }}</h1>
      <p class="page-sub">{{ t('dashboard.sub') }}</p>
    </header>

    <div
      v-if="loadErrorMessage"
      class="mb-4 rounded-md bg-danger/10 px-3.5 py-2.5 text-[13px] text-danger"
    >
      {{ loadErrorMessage }}
    </div>

    <div v-if="showSkeleton" class="mb-8 grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
      <SkeletonCard :count="3" :show-image="false" />
    </div>

    <div v-else-if="data" class="mb-8 grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
      <div class="card p-4">
        <div class="mb-2 flex items-center gap-1.5 text-xs font-medium text-text-muted">
          <PhKey :size="18" weight="regular" />
          <span class="text-[11px] uppercase tracking-wide">{{ t('dashboard.profiles') }}</span>
        </div>
        <div class="font-mono text-[28px] font-bold leading-none tabular-nums">{{ profiles.length }}</div>
        <div class="mt-1.5 font-mono text-[11px] text-text-dim">
          {{ t('dashboard.biDirectional', { n: profiles.filter(p => p.direction === 'bi' || p.direction === 'bi-resync').length }) }}
        </div>
      </div>

      <div class="card p-4">
        <div class="mb-2 flex items-center gap-1.5 text-xs font-medium text-text-muted">
          <PhCloud :size="18" weight="regular" />
          <span class="text-[11px] uppercase tracking-wide">{{ t('dashboard.remotes') }}</span>
        </div>
        <div class="font-mono text-[28px] font-bold leading-none tabular-nums">{{ remotes.length }}</div>
        <div class="mt-1.5 font-mono text-[11px] text-text-dim">
          {{ t('dashboard.uniqueProviders', { n: new Set(remotes.map(r => r.type)).size }) }}
        </div>
      </div>

      <div class="card p-4">
        <div class="mb-2 flex items-center gap-1.5 text-xs font-medium text-text-muted">
          <PhCircleNotch :size="18" weight="regular" />
          <span class="text-[11px] uppercase tracking-wide">{{ t('dashboard.activeTasks') }}</span>
        </div>
        <div class="font-mono text-[28px] font-bold leading-none tabular-nums">{{ activeTasks.length }}</div>
        <div v-if="activeTasks.length > 0" class="mt-1.5 font-mono text-[11px] text-text-dim">
          {{ activeTasks[0].name }} ({{ activeTasks[0].action }})
        </div>
        <div v-else class="mt-1.5 font-mono text-[11px] text-text-dim">{{ t('dashboard.idle') }}</div>
      </div>
    </div>

    <section class="mt-6">
      <h2 class="section-label">{{ t('dashboard.recentActivity') }}</h2>
      <div v-if="activeTasks.length === 0">
        <EmptyState
          :title="t('dashboard.noSyncs')"
          :description="t('dashboard.noSyncsDesc')"
        />
      </div>
      <div v-else class="space-y-2">
        <div
          v-for="task in activeTasks"
          :key="task.id"
          class="card flex items-center gap-2 px-4 py-3 text-[13px]"
        >
          <PhStack :size="16" weight="regular" class="text-accent" />
          <span class="font-medium">{{ task.name }}</span>
          <span class="text-text-dim">({{ task.action }})</span>
          <span class="ml-auto font-mono text-[11px] text-text-dim">{{ task.status }}</span>
        </div>
      </div>
    </section>

    <section class="mt-6">
      <h2 class="section-label">{{ t('dashboard.quickLinks') }}</h2>
      <div class="flex flex-wrap gap-2">
        <RouterLink :to="{ name: 'profiles' }" class="quick-link" data-testid="dashboard-quick-link-profiles">
          <PhKey :size="16" weight="regular" />
          <span>{{ t('dashboard.manageProfiles') }}</span>
        </RouterLink>
        <RouterLink :to="{ name: 'remotes' }" class="quick-link" data-testid="dashboard-quick-link-remotes">
          <PhCloud :size="16" weight="regular" />
          <span>{{ t('dashboard.configureRemotes') }}</span>
        </RouterLink>
        <RouterLink :to="{ name: 'flows' }" class="quick-link" data-testid="dashboard-quick-link-flows">
          <PhStack :size="16" weight="regular" />
          <span>{{ t('dashboard.manageFlows') }}</span>
        </RouterLink>
      </div>
    </section>
  </div>
</template>
