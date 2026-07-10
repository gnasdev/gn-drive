<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import {
  PhSun,
  PhMoon,
  PhLock,
  PhCircle,
  PhCloud,
  PhGearSix,
  PhSquaresFour,
} from '@phosphor-icons/vue'
import { useAuthStore } from '@/stores/auth'
import { useThemeStore } from '@/stores/theme'
import { api } from '@/api/client'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { useToast } from '@/composables/useToast'
import { cn } from '@/lib/cn'

const { t } = useI18n()
const auth = useAuthStore()
const theme = useThemeStore()
const router = useRouter()
const route = useRoute()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()

const online = ref(false)
const checking = ref(true)

async function checkHealth() {
  try {
    await api.get('/api/v1/status')
    online.value = true
  } catch {
    online.value = false
  } finally {
    checking.value = false
  }
}

onMounted(() => {
  checkHealth()
  setInterval(checkHealth, 15000)
})

async function onLock() {
  const ok = await confirmDialog({
    title: t('topbar.lockTitle'),
    message: t('topbar.lockMessage'),
    confirmText: t('topbar.lock'),
  })
  if (!ok) return
  try {
    await auth.lock()
    router.push({ name: 'unlock' })
  } catch (e) {
    toast.error((e as Error).message)
  }
}

function goWorkspace() {
  router.push({ name: 'workspace' })
}

function goSettings() {
  router.push({ name: 'settings' })
}
</script>

<template>
  <header
    class="flex h-[var(--topbar-height)] shrink-0 items-center gap-3 border-b-2 border-border bg-accent px-4"
    data-testid="app-topbar"
  >
    <button
      type="button"
      class="flex items-center gap-2 font-bold text-text"
      data-testid="nav-workspace"
      @click="goWorkspace"
    >
      <PhCloud :size="22" weight="bold" />
      <span class="text-lg">GN Drive</span>
    </button>

    <div class="flex items-center gap-1.5 text-xs font-bold text-text/80">
      <span
        :class="cn(
          'flex',
          online && 'text-success',
          checking && 'animate-pulse text-warning',
          !online && !checking && 'text-text-dim',
        )"
      >
        <PhCircle :size="8" weight="fill" />
      </span>
      <span>
        <template v-if="checking">{{ t('topbar.connecting') }}</template>
        <template v-else-if="online">{{ t('topbar.connected') }}</template>
        <template v-else>{{ t('topbar.offline') }}</template>
      </span>
    </div>

    <div class="flex-1" />

    <button
      type="button"
      class="btn-ghost"
      :class="route.name === 'workspace' && 'bg-text/10'"
      data-testid="nav-workspace-btn"
      :title="t('nav.workspace')"
      @click="goWorkspace"
    >
      <PhSquaresFour :size="18" weight="bold" />
      <span class="hidden sm:inline">{{ t('nav.workspace') }}</span>
    </button>

    <button
      type="button"
      class="btn-ghost"
      :class="route.name === 'settings' && 'bg-text/10'"
      data-testid="nav-settings"
      :title="t('nav.settings')"
      @click="goSettings"
    >
      <PhGearSix :size="18" weight="bold" />
      <span class="hidden sm:inline">{{ t('nav.settings') }}</span>
    </button>

    <button
      class="btn-icon"
      :title="t('topbar.theme', { pref: theme.preference })"
      data-testid="theme-toggle"
      @click="theme.setTheme(theme.isDark ? 'light' : 'dark')"
    >
      <PhSun v-if="theme.isDark" :size="18" weight="bold" />
      <PhMoon v-else :size="18" weight="bold" />
    </button>

    <button class="btn-icon" :title="t('topbar.lock')" data-testid="lock-button" @click="onLock">
      <PhLock :size="18" weight="bold" />
    </button>
  </header>
</template>
