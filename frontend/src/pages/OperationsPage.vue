<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhPlay } from '@phosphor-icons/vue'
import { useOperationsStore } from '@/stores/operations'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppAlert from '@/components/ui/Alert.vue'
import { useToast } from '@/composables/useToast'
import { SYNC_ACTIONS, type SyncAction } from '@/constants/forms'
import { cn } from '@/lib/cn'

const { t } = useI18n()
const store = useOperationsStore()
const toast = useToast()

const lastTaskId = ref<string | null>(null)
const syncProfile = ref('')

const selectedProfile = computed(() =>
  store.profiles.find((p) => p.name === syncProfile.value) ?? null,
)

onMounted(async () => {
  await Promise.all([store.loadProfiles(), store.loadTasks()])
  if (store.profiles.length > 0) {
    syncProfile.value = store.profiles[0].name
  }
})

async function doStartSync(action: SyncAction) {
  const profileName = syncProfile.value || store.profiles[0]?.name
  if (!profileName) {
    toast.error(t('operations.createProfileFirst'))
    return
  }
  lastTaskId.value = await store.startSync(action, profileName)
  if (lastTaskId.value) {
    setTimeout(() => store.loadTasks(), 500)
  }
}
</script>

<template>
  <div class="page-shell" data-testid="page-operations">
    <header class="mb-5">
      <h1 class="page-title">{{ t('operations.title') }}</h1>
      <p class="page-sub">{{ t('operations.sub') }}</p>
    </header>

    <section class="card mb-3 px-5 py-4">
      <h3 class="section-label">{{ t('operations.quickSync') }}</h3>
      <p class="mb-2 text-xs text-text-dim">{{ t('operations.quickSyncHelp') }}</p>

      <label class="field-label mb-2.5 max-w-sm">
        <span>{{ t('operations.profile') }}</span>
        <select v-model="syncProfile" class="field-input" data-testid="ops-sync-profile">
          <option value="" disabled>{{ t('common.selectProfile') }}</option>
          <option v-for="p in store.profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
        </select>
      </label>

      <div
        v-if="selectedProfile"
        class="mb-3 rounded-md border border-border bg-bg px-3 py-2 font-mono text-[11px] text-text-muted"
        data-testid="ops-sync-profile-paths"
      >
        <span class="text-text-dim">{{ t('operations.from') }}</span> {{ selectedProfile.from || t('common.empty') }}
        <span class="mx-1.5 text-text-dim">→</span>
        <span class="text-text-dim">{{ t('operations.to') }}</span> {{ selectedProfile.to || t('common.empty') }}
        <span v-if="selectedProfile.direction" class="ml-2 badge">
          {{ t('operations.defaultDirection', { dir: selectedProfile.direction }) }}
        </span>
      </div>

      <div class="mb-3 flex flex-wrap gap-1.5">
        <button
          v-for="a in SYNC_ACTIONS"
          :key="a"
          type="button"
          class="btn-ghost font-mono"
          :data-testid="`ops-sync-${a}`"
          @click="doStartSync(a)"
        >
          <PhPlay :size="14" weight="bold" /> {{ a }}
        </button>
      </div>

      <details class="mt-1" data-testid="ops-sync-help">
        <summary class="cursor-pointer text-[11px] font-medium text-text-dim hover:text-text-muted">
          {{ t('operations.allSyncGlance') }}
        </summary>
        <ul class="mt-2 grid list-none gap-2 p-0 sm:grid-cols-2">
          <li
            v-for="a in SYNC_ACTIONS"
            :key="a"
            class="rounded-md border border-border bg-bg px-3 py-2.5"
          >
            <div class="font-mono text-[12px] font-semibold text-text">{{ a }}</div>
            <p class="m-0 mt-1 text-[12px] font-medium leading-snug text-text">
              {{ t(`syncHelp.${a}.title`) }}
            </p>
            <p class="m-0 mt-1 text-[11px] leading-relaxed text-text-muted">
              {{ t(`syncHelp.${a}.body`) }}
            </p>
          </li>
        </ul>
      </details>

      <AppAlert v-if="lastTaskId" type="success" class="mt-3" data-testid="ops-task-started">
        {{ t('operations.startedTask') }} <code class="font-mono">{{ lastTaskId }}</code>
      </AppAlert>
    </section>

    <section class="card mb-3 px-5 py-4">
      <h3 class="section-label">{{ t('operations.activeTasks') }}</h3>
      <p class="mb-2 text-xs text-text-dim">{{ t('operations.activeTasksHelp') }}</p>
      <div v-if="store.tasks.length === 0"><EmptyState :title="t('operations.noTasks')" /></div>
      <div v-else class="flex flex-col gap-1.5" data-testid="ops-task-list">
        <div
          v-for="task in store.tasks"
          :key="task.id"
          class="flex items-center justify-between rounded-md bg-bg px-2.5 py-2"
        >
          <div class="text-[13px]">
            {{ task.name }} <span class="badge ml-1">{{ task.action }}</span>
          </div>
          <div class="flex items-center gap-2">
            <span
              :class="cn(
                'rounded px-1.5 py-px text-[11px]',
                task.status === 'running' && 'bg-running/15 text-running',
                task.status === 'completed' && 'bg-completed/15 text-completed',
                task.status === 'failed' && 'bg-failed/15 text-failed',
                task.status === 'cancelled' && 'bg-warning/15 text-warning',
              )"
            >
              {{ task.status }}
            </span>
            <span v-if="task.transferred" class="font-mono text-[11px]">{{ task.transferred }} B</span>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>
