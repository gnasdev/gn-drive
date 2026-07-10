<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { PhSun, PhMoon, PhLock, PhCircle } from '@phosphor-icons/vue'
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
</script>

<template>
  <header
    class="flex h-[var(--topbar-height)] items-center gap-2 border-b border-border bg-surface px-4"
  >
    <div class="flex items-center gap-1.5 px-2 text-xs text-text-muted">
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
      class="btn-icon"
      :title="t('topbar.theme', { pref: theme.preference })"
      data-testid="theme-toggle"
      @click="theme.setTheme(theme.isDark ? 'light' : 'dark')"
    >
      <PhSun v-if="theme.isDark" :size="18" weight="regular" />
      <PhMoon v-else :size="18" weight="regular" />
    </button>

    <button class="btn-icon" :title="t('topbar.lock')" data-testid="lock-button" @click="onLock">
      <PhLock :size="18" weight="regular" />
    </button>
  </header>
</template>
