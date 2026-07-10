<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import {
  PhHouse,
  PhKey,
  PhCloud,
  PhSwap,
  PhSquaresFour,
  PhStack,
  PhCalendar,
  PhClockCounterClockwise,
  PhGearSix,
  PhCircleNotch,
} from '@phosphor-icons/vue'
import { cn } from '@/lib/cn'

interface NavItem {
  name: string
  labelKey: string
  icon: any
  group: 'main' | 'data' | 'work' | 'system'
}

const { t } = useI18n()

const items: NavItem[] = [
  { name: 'dashboard', labelKey: 'nav.dashboard', icon: PhHouse, group: 'main' },
  { name: 'profiles', labelKey: 'nav.profiles', icon: PhKey, group: 'data' },
  { name: 'remotes', labelKey: 'nav.remotes', icon: PhCloud, group: 'data' },
  { name: 'operations', labelKey: 'nav.operations', icon: PhSwap, group: 'work' },
  { name: 'boards', labelKey: 'nav.boards', icon: PhSquaresFour, group: 'work' },
  { name: 'flows', labelKey: 'nav.flows', icon: PhStack, group: 'work' },
  { name: 'schedules', labelKey: 'nav.schedules', icon: PhCalendar, group: 'work' },
  { name: 'history', labelKey: 'nav.history', icon: PhClockCounterClockwise, group: 'work' },
  { name: 'service', labelKey: 'nav.service', icon: PhCircleNotch, group: 'system' },
  { name: 'settings', labelKey: 'nav.settings', icon: PhGearSix, group: 'system' },
]

const route = useRoute()
const current = computed(() => route.name as string)
const groups = computed(() => {
  const out: Record<string, NavItem[]> = { main: [], data: [], work: [], system: [] }
  for (const it of items) {
    out[it.group].push(it)
  }
  return out
})
</script>

<template>
  <aside
    class="flex h-dvh w-[var(--sidebar-width)] shrink-0 flex-col overflow-hidden border-r border-border bg-surface"
  >
    <div class="flex items-center gap-2.5 border-b border-border p-4">
      <div
        class="flex h-8 w-8 items-center justify-center rounded-md bg-accent text-[13px] font-bold tracking-wide text-white"
      >
        GN
      </div>
      <div class="leading-tight">
        <div class="text-sm font-semibold">GN Drive</div>
        <div class="text-[11px] text-text-dim">{{ t('nav.brandSub') }}</div>
      </div>
    </div>

    <nav class="flex-1 overflow-y-auto px-2 py-3">
      <div v-for="(groupItems, group) in groups" :key="group" class="mb-4">
        <div
          v-if="groupItems.length"
          class="px-3 pb-1.5 pt-1 text-[10px] font-semibold uppercase tracking-wider text-text-dim"
        >
          {{ t(`nav.groups.${group}`) }}
        </div>
        <RouterLink
          v-for="item in groupItems"
          :key="item.name"
          :to="{ name: item.name }"
          :class="cn(
            'flex items-center gap-2.5 rounded-md px-3 py-[7px] text-[13px] font-medium text-text-muted transition-colors hover:bg-surface-hover hover:text-text',
            current === item.name && 'bg-accent/12 text-accent',
          )"
          :data-testid="`nav-${item.name}`"
        >
          <component :is="item.icon" :size="18" weight="regular" />
          <span>{{ t(item.labelKey) }}</span>
        </RouterLink>
      </div>
    </nav>

    <div class="border-t border-border px-4 py-3">
      <span class="font-mono text-[11px] text-text-dim">v1.0.0</span>
    </div>
  </aside>
</template>
