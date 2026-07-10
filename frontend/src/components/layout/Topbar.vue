<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { PhSun, PhMoon, PhLock, PhCircle } from '@phosphor-icons/vue'
import { useAuthStore } from '@/stores/auth'
import { useThemeStore } from '@/stores/theme'
import { api } from '@/api/client'
import { useConfirmDialog, useToast } from '@gnas/ui-shared'

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
    title: 'Lock app',
    message: 'Lock the app? You will need your master password to unlock again.',
    confirmText: 'Lock',
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
  <header class="topbar">
    <div class="status">
      <span class="status-dot" :class="{ online, checking }">
        <PhCircle :size="8" weight="fill" />
      </span>
      <span class="status-text">
        <template v-if="checking">connecting…</template>
        <template v-else-if="online">connected</template>
        <template v-else>offline</template>
      </span>
    </div>

    <div class="spacer" />

    <button
      class="icon-btn"
      :title="`theme: ${theme.preference}`"
      data-testid="theme-toggle"
      @click="theme.setTheme(theme.isDark ? 'light' : 'dark')"
    >
      <PhSun v-if="theme.isDark" :size="18" weight="regular" />
      <PhMoon v-else :size="18" weight="regular" />
    </button>

    <button class="icon-btn" title="Lock" data-testid="lock-button" @click="onLock">
      <PhLock :size="18" weight="regular" />
    </button>
  </header>
</template>

<style scoped>
.topbar {
  height: var(--topbar-height);
  background: var(--color-surface);
  border-bottom: 1px solid var(--color-border);
  display: flex;
  align-items: center;
  padding: 0 16px;
  gap: 8px;
}
.status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--color-text-muted);
  padding: 0 8px;
}
.status-dot {
  display: flex;
  color: var(--color-text-dim);
}
.status-dot.online { color: var(--color-success); }
.status-dot.checking {
  color: var(--color-warning);
  animation: pulse 1s ease-in-out infinite;
}
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}
.spacer { flex: 1; }
.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px; height: 32px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 6px;
  color: var(--color-text-muted);
  transition: background-color 0.1s, color 0.1s, border-color 0.1s;
}
.icon-btn:hover {
  background: var(--color-surface-hover);
  color: var(--color-text);
  border-color: var(--color-border);
}
</style>
