<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhGearSix, PhSun, PhMoon, PhKey, PhLock, PhDownloadSimple } from '@phosphor-icons/vue'
import { useThemeStore } from '@/stores/theme'
import { useAuthStore } from '@/stores/auth'
import { useApi } from '@/composables/useApi'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'

const theme = useThemeStore()
const auth = useAuthStore()
const api = useApi()

const settings = ref<Record<string, string>>({})
const newPwd = ref('')
const oldPwd = ref('')
const msg = ref<{ kind: 'ok' | 'err'; text: string } | null>(null)
const updateMsg = ref<string>('')

onMounted(async () => {
  settings.value = (await api.get<Record<string, string>>('/api/v1/settings')) ?? {}
})

async function saveSetting(key: string, value: string) {
  await api.post('/api/v1/settings', { [key]: value })
  settings.value = (await api.get<Record<string, string>>('/api/v1/settings')) ?? {}
  msg.value = { kind: 'ok', text: `Saved ${key}.` }
  setTimeout(() => (msg.value = null), 2000)
}

async function changePassword() {
  if (newPwd.value.length < 4) {
    msg.value = { kind: 'err', text: 'New password must be at least 4 characters.' }
    return
  }
  try {
    await api.post('/api/v1/auth/change-password', { old_password: oldPwd.value, new_password: newPwd.value })
    msg.value = { kind: 'ok', text: 'Password changed.' }
    newPwd.value = ''
    oldPwd.value = ''
  } catch (e: any) {
    msg.value = { kind: 'err', text: e?.message ?? 'change failed' }
  }
}

async function selfUpdate() {
  updateMsg.value = 'Checking for updates…'
  try {
    const r = await fetch('/api/v1/self-update', { method: 'POST', credentials: 'same-origin' })
    const j = await r.json()
    updateMsg.value = j.output ?? JSON.stringify(j)
  } catch (e: any) {
    updateMsg.value = e?.message ?? 'update failed'
  }
}
</script>

<template>
  <div class="settings-page">
    <header class="page-header">
      <h1>Settings</h1>
      <p class="sub">App preferences, master password, and self-update.</p>
    </header>

    <AppAlert v-if="msg" :type="msg.kind === 'ok' ? 'success' : 'error'">{{ msg.text }}</AppAlert>

    <section class="card">
      <h2><PhSun :size="14" weight="bold" /> Appearance</h2>
      <div class="row">
        <div>
          <div class="row-label">Theme</div>
          <div class="row-help">Dark by default; light via this toggle.</div>
        </div>
        <div class="row-actions">
          <button class="toggle" :class="{ on: theme.preference === 'dark' }" @click="theme.setTheme('dark')">
            <PhMoon :size="14" weight="bold" /> Dark
          </button>
          <button class="toggle" :class="{ on: theme.preference === 'light' }" @click="theme.setTheme('light')">
            <PhSun :size="14" weight="bold" /> Light
          </button>
        </div>
      </div>
    </section>

    <section class="card">
      <h2><PhKey :size="14" weight="bold" /> Master password</h2>
      <div class="form-grid">
        <label>
          <span>Current password</span>
          <input v-model="oldPwd" type="password" autocomplete="current-password" />
        </label>
        <label>
          <span>New password</span>
          <input v-model="newPwd" type="password" autocomplete="new-password" />
        </label>
      </div>
      <div class="form-actions">
        <button class="primary" :disabled="!oldPwd || !newPwd" @click="changePassword">Change password</button>
      </div>
    </section>

    <section class="card">
      <h2><PhLock :size="14" weight="bold" /> Lock now</h2>
      <p class="row-help">Encrypt config files and require password to unlock.</p>
      <div class="form-actions">
        <button class="danger" @click="auth.lock()">Lock app</button>
      </div>
    </section>

    <section class="card">
      <h2><PhDownloadSimple :size="14" weight="bold" /> Self-update</h2>
      <p class="row-help">Download and apply the latest release from GitHub.</p>
      <div class="form-actions">
        <button class="primary" @click="selfUpdate">Check &amp; install</button>
      </div>
      <pre v-if="updateMsg" class="update-out">{{ updateMsg }}</pre>
    </section>
  </div>
</template>

<style scoped>
.settings-page { max-width: 800px; margin: 0 auto; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0 0 20px; }
.banner { padding: 8px 12px; border-radius: 6px; font-size: 13px; margin-bottom: 12px; }
.banner.ok { background: color-mix(in srgb, var(--color-success) 12%, transparent); color: var(--color-success); }
.banner.err { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); }

.card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 8px; padding: 18px 20px; margin-bottom: 12px; }
.card h2 { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; margin: 0 0 14px; }

.row { display: flex; justify-content: space-between; align-items: center; gap: 16px; }
.row-label { font-size: 13px; font-weight: 500; }
.row-help { font-size: 12px; color: var(--color-text-dim); margin-top: 2px; }
.row-actions { display: flex; gap: 6px; }
.toggle { display: inline-flex; align-items: center; gap: 4px; padding: 5px 10px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text-muted); font-size: 12px; }
.toggle.on { color: var(--color-accent); border-color: color-mix(in srgb, var(--color-accent) 40%, transparent); }
.toggle:hover { background: var(--color-surface-hover); }

.form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 12px; }
.form-grid label { display: flex; flex-direction: column; gap: 4px; }
.form-grid label span { font-size: 11px; color: var(--color-text-muted); font-weight: 500; }
.form-grid input { padding: 7px 10px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-family: var(--font-mono); font-size: 13px; }
.form-grid input:focus { outline: none; border-color: var(--color-accent); box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent); }
.form-actions { display: flex; justify-content: flex-end; gap: 6px; }
.primary { padding: 7px 14px; background: var(--color-accent); color: white; border: 0; border-radius: 6px; font-size: 13px; font-weight: 500; }
.primary:disabled { opacity: 0.5; }
.danger { padding: 7px 14px; background: transparent; border: 1px solid var(--color-border); border-radius: 6px; color: var(--color-text); font-size: 13px; }
.danger:hover { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); border-color: color-mix(in srgb, var(--color-danger) 30%, transparent); }
.update-out { margin-top: 12px; padding: 10px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 6px; font-family: var(--font-mono); font-size: 11px; color: var(--color-text-muted); white-space: pre-wrap; max-height: 200px; overflow: auto; }
</style>
