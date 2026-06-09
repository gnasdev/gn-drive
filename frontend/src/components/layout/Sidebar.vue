<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, RouterLink } from 'vue-router'
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

interface NavItem {
  name: string
  label: string
  icon: any
  group?: string
}

const items: NavItem[] = [
  { name: 'dashboard',  label: 'Dashboard',   icon: PhHouse,           group: 'main' },
  { name: 'profiles',   label: 'Profiles',    icon: PhKey,             group: 'data' },
  { name: 'remotes',    label: 'Remotes',     icon: PhCloud,           group: 'data' },
  { name: 'operations', label: 'Operations',  icon: PhSwap,            group: 'work' },
  { name: 'boards',     label: 'Boards',      icon: PhSquaresFour,     group: 'work' },
  { name: 'flows',      label: 'Flows',       icon: PhStack,           group: 'work' },
  { name: 'schedules',  label: 'Schedules',   icon: PhCalendar,        group: 'work' },
  { name: 'history',    label: 'History',     icon: PhClockCounterClockwise, group: 'work' },
  { name: 'service',    label: 'Service',     icon: PhCircleNotch,     group: 'system' },
  { name: 'settings',   label: 'Settings',    icon: PhGearSix,         group: 'system' },
]

const route = useRoute()
const current = computed(() => route.name as string)
const groups = computed(() => {
  const out: Record<string, NavItem[]> = { main: [], data: [], work: [], system: [] }
  for (const it of items) {
    const g = it.group ?? 'main'
    out[g] = out[g] || []
    out[g].push(it)
  }
  return out
})
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">GN</div>
      <div class="brand-text">
        <div class="brand-name">GN Drive</div>
        <div class="brand-sub">sync engine</div>
      </div>
    </div>

    <nav class="nav">
      <div v-for="(items, group) in groups" :key="group" class="nav-group">
        <div v-if="items.length" class="nav-group-label">{{ group }}</div>
        <RouterLink
          v-for="item in items"
          :key="item.name"
          :to="{ name: item.name }"
          class="nav-item"
          :class="{ active: current === item.name }"
        >
          <component :is="item.icon" :size="18" weight="regular" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </div>
    </nav>

    <div class="footer">
      <span class="version">v{{ $store ? '' : '' }}</span>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  flex-shrink: 0;
  background: var(--color-surface);
  border-right: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 16px;
  border-bottom: 1px solid var(--color-border);
}
.brand-mark {
  width: 32px; height: 32px;
  background: var(--color-accent);
  color: white;
  border-radius: 6px;
  display: flex; align-items: center; justify-content: center;
  font-weight: 700;
  font-size: 13px;
  letter-spacing: 0.5px;
}
.brand-text { line-height: 1.2; }
.brand-name { font-weight: 600; font-size: 14px; }
.brand-sub { color: var(--color-text-dim); font-size: 11px; }

.nav {
  flex: 1;
  overflow-y: auto;
  padding: 12px 8px;
}

.nav-group {
  margin-bottom: 16px;
}
.nav-group-label {
  font-size: 10px;
  font-weight: 600;
  color: var(--color-text-dim);
  text-transform: uppercase;
  letter-spacing: 0.6px;
  padding: 4px 12px 6px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 12px;
  border-radius: 6px;
  color: var(--color-text-muted);
  font-size: 13px;
  font-weight: 500;
  transition: background-color 0.1s, color 0.1s;
}
.nav-item:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
}
.nav-item.active {
  background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  color: var(--color-accent);
}

.footer {
  padding: 12px 16px;
  border-top: 1px solid var(--color-border);
}
.version {
  color: var(--color-text-dim);
  font-size: 11px;
  font-family: var(--font-mono);
}
</style>
