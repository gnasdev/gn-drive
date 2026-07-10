<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { PhKey, PhSun, PhMoon, PhLock, PhDownloadSimple, PhGlobe } from '@phosphor-icons/vue'
import { useThemeStore } from '@/stores/theme'
import { useLocaleStore } from '@/stores/locale'
import { useAuthStore } from '@/stores/auth'
import { useApi } from '@/composables/useApi'
import AppAlert from '@/components/ui/Alert.vue'
import type { AppLocale } from '@/i18n'

const { t } = useI18n()
const theme = useThemeStore()
const localeStore = useLocaleStore()
const auth = useAuthStore()
const api = useApi()
const router = useRouter()

const settings = ref<Record<string, string>>({})
const newPwd = ref('')
const oldPwd = ref('')
const msg = ref<{ kind: 'ok' | 'err'; text: string } | null>(null)
const updateMsg = ref<string>('')

onMounted(async () => {
  settings.value = (await api.get<Record<string, string>>('/api/v1/settings')) ?? {}
})

async function changePassword() {
  if (newPwd.value.length < 4) {
    msg.value = { kind: 'err', text: t('settings.pwdTooShort') }
    return
  }
  try {
    await api.post('/api/v1/auth/change-password', {
      old_password: oldPwd.value,
      new_password: newPwd.value,
    })
    auth.unlocked = false
    msg.value = { kind: 'ok', text: t('settings.pwdChanged') }
    newPwd.value = ''
    oldPwd.value = ''
    await router.push({ name: 'unlock' })
  } catch (e: any) {
    msg.value = { kind: 'err', text: e?.message ?? 'change failed' }
  }
}

async function selfUpdate() {
  updateMsg.value = t('settings.checkingUpdate')
  try {
    const r = await fetch('/api/v1/self-update', { method: 'POST', credentials: 'same-origin' })
    const j = await r.json()
    updateMsg.value = j.output ?? JSON.stringify(j)
  } catch (e: any) {
    updateMsg.value = e?.message ?? 'update failed'
  }
}

async function lockApp() {
  try {
    await auth.lock()
    await router.push({ name: 'unlock' })
  } catch {
    // error already in store
  }
}

function setLang(code: AppLocale) {
  localeStore.setLocale(code)
}
</script>

<template>
  <div class="mx-auto max-w-[800px]" data-testid="page-settings">
    <header class="mb-5">
      <h1 class="page-title">{{ t('settings.title') }}</h1>
      <p class="page-sub">{{ t('settings.sub') }}</p>
    </header>

    <AppAlert
      v-if="msg"
      :type="msg.kind === 'ok' ? 'success' : 'error'"
      data-testid="settings-msg"
    >
      {{ msg.text }}
    </AppAlert>

    <section class="card mb-3 px-5 py-4.5">
      <h2 class="mb-3.5 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-text-muted">
        <PhSun :size="14" weight="bold" /> {{ t('settings.appearance') }}
      </h2>
      <div class="flex items-center justify-between gap-4">
        <div>
          <div class="text-[13px] font-medium">{{ t('settings.theme') }}</div>
          <div class="mt-0.5 text-xs text-text-dim">{{ t('settings.themeHelp') }}</div>
        </div>
        <div class="flex gap-1.5">
          <button
            class="toggle"
            :class="{ on: theme.preference === 'dark' }"
            data-testid="theme-dark"
            @click="theme.setTheme('dark')"
          >
            <PhMoon :size="14" weight="bold" /> {{ t('settings.dark') }}
          </button>
          <button
            class="toggle"
            :class="{ on: theme.preference === 'light' }"
            data-testid="theme-light"
            @click="theme.setTheme('light')"
          >
            <PhSun :size="14" weight="bold" /> {{ t('settings.light') }}
          </button>
        </div>
      </div>
    </section>

    <section class="card mb-3 px-5 py-4.5">
      <h2 class="mb-3.5 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-text-muted">
        <PhGlobe :size="14" weight="bold" /> {{ t('settings.language') }}
      </h2>
      <div class="flex items-center justify-between gap-4">
        <div>
          <div class="text-[13px] font-medium">{{ t('settings.language') }}</div>
          <div class="mt-0.5 text-xs text-text-dim">{{ t('settings.languageHelp') }}</div>
        </div>
        <div class="flex gap-1.5">
          <button
            class="toggle"
            :class="{ on: localeStore.locale === 'en' }"
            data-testid="lang-en"
            @click="setLang('en')"
          >
            {{ t('settings.english') }}
          </button>
          <button
            class="toggle"
            :class="{ on: localeStore.locale === 'vi' }"
            data-testid="lang-vi"
            @click="setLang('vi')"
          >
            {{ t('settings.vietnamese') }}
          </button>
        </div>
      </div>
    </section>

    <section class="card mb-3 px-5 py-4.5">
      <h2 class="mb-3.5 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-text-muted">
        <PhKey :size="14" weight="bold" /> {{ t('settings.masterPassword') }}
      </h2>
      <div class="mb-3 grid grid-cols-1 gap-2.5 md:grid-cols-2">
        <label class="field-label">
          <span>{{ t('settings.currentPassword') }}</span>
          <input
            v-model="oldPwd"
            type="password"
            autocomplete="current-password"
            class="field-input"
            data-testid="settings-old-password"
          />
        </label>
        <label class="field-label">
          <span>{{ t('settings.newPassword') }}</span>
          <input
            v-model="newPwd"
            type="password"
            autocomplete="new-password"
            class="field-input"
            data-testid="settings-new-password"
          />
        </label>
      </div>
      <div class="flex justify-end">
        <button
          class="btn-primary"
          :disabled="!oldPwd || !newPwd"
          data-testid="settings-change-password"
          @click="changePassword"
        >
          {{ t('settings.changePassword') }}
        </button>
      </div>
    </section>

    <section class="card mb-3 px-5 py-4.5">
      <h2 class="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-text-muted">
        <PhLock :size="14" weight="bold" /> {{ t('settings.lockNow') }}
      </h2>
      <p class="mb-3 text-xs text-text-dim">{{ t('settings.lockHelp') }}</p>
      <div class="flex justify-end">
        <button class="danger !px-3.5 !py-1.5" data-testid="settings-lock" @click="lockApp">
          {{ t('settings.lockApp') }}
        </button>
      </div>
    </section>

    <section class="card mb-3 px-5 py-4.5">
      <h2 class="mb-2 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-text-muted">
        <PhDownloadSimple :size="14" weight="bold" /> {{ t('settings.selfUpdate') }}
      </h2>
      <p class="mb-3 text-xs text-text-dim">{{ t('settings.selfUpdateHelp') }}</p>
      <div class="flex justify-end">
        <button class="btn-primary" @click="selfUpdate">{{ t('settings.checkInstall') }}</button>
      </div>
      <pre
        v-if="updateMsg"
        class="mt-3 max-h-[200px] overflow-auto whitespace-pre-wrap rounded-md border border-border bg-bg p-2.5 font-mono text-[11px] text-text-muted"
      >{{ updateMsg }}</pre>
    </section>
  </div>
</template>
