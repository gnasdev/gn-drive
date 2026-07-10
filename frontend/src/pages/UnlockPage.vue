<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAuthStore } from '@/stores/auth'
import { useToast } from '@/composables/useToast'
import AppAlert from '@/components/ui/Alert.vue'

const { t } = useI18n()
const auth = useAuthStore()
const router = useRouter()
const toast = useToast()

const password = ref('')
const confirm = ref('')
const mode = computed<'setup' | 'unlock'>(() => (auth.setup ? 'unlock' : 'setup'))

onMounted(async () => {
  if (!auth.initialized) await auth.fetchStatus()
})

async function submit() {
  if (mode.value === 'setup' && password.value !== confirm.value) {
    return toast.error(t('unlock.mismatch'))
  }
  if (password.value.length < 4) {
    return toast.error(t('unlock.tooShort'))
  }
  try {
    if (mode.value === 'setup') {
      await auth.doSetup(password.value)
    } else {
      await auth.unlock(password.value)
    }
    // Route home is `workspace` (single-page shell); use replace so Back
    // does not return to the unlock form after a successful session.
    await router.replace({ name: 'workspace' })
  } catch {
    // error already in store
  }
}
</script>

<template>
  <div
    class="flex min-h-dvh w-full items-center justify-center bg-bg p-6"
    data-testid="page-unlock"
  >
    <div class="w-full max-w-[380px] rounded-[10px] border border-border bg-surface px-7 py-8">
      <div class="mb-6 flex items-center gap-2.5">
        <div
          class="flex h-8 w-8 items-center justify-center rounded-md bg-accent text-[13px] font-bold text-white"
        >
          GN
        </div>
        <div class="font-semibold">GN Drive</div>
      </div>

      <h1 class="mb-2 text-lg font-semibold" data-testid="unlock-title">
        {{ mode === 'setup' ? t('unlock.setupTitle') : t('unlock.unlockTitle') }}
      </h1>
      <p class="mb-5 text-[13px] leading-relaxed text-text-muted">
        <template v-if="mode === 'setup'">
          <i18n-t keypath="unlock.setupBody" tag="span">
            <template #db>
              <code class="rounded bg-surface-hover px-1 font-mono text-xs">gn-drive.db</code>
            </template>
            <template #conf>
              <code class="rounded bg-surface-hover px-1 font-mono text-xs">rclone.conf</code>
            </template>
          </i18n-t>
        </template>
        <template v-else>
          {{ t('unlock.unlockBody') }}
        </template>
      </p>

      <form class="flex flex-col gap-3.5" data-testid="unlock-form" @submit.prevent="submit">
        <label class="field-label">
          <span>{{ t('unlock.password') }}</span>
          <input
            v-model="password"
            type="password"
            autofocus
            autocomplete="current-password"
            class="field-input !font-sans"
            :disabled="auth.busy"
            data-testid="unlock-password"
          />
        </label>

        <label v-if="mode === 'setup'" class="field-label">
          <span>{{ t('unlock.confirm') }}</span>
          <input
            v-model="confirm"
            type="password"
            autocomplete="new-password"
            class="field-input !font-sans"
            :disabled="auth.busy"
            data-testid="unlock-confirm"
          />
        </label>

        <AppAlert v-if="auth.error" type="error" data-testid="unlock-error">{{ auth.error }}</AppAlert>

        <button
          type="submit"
          class="btn-primary mt-1 w-full justify-center py-2.5 font-semibold"
          :disabled="auth.busy || !password"
          data-testid="unlock-submit"
        >
          {{ auth.busy ? '…' : mode === 'setup' ? t('unlock.submitSetup') : t('unlock.submitUnlock') }}
        </button>
      </form>

      <div class="mt-6 text-center font-mono text-[11px] text-text-dim">
        {{ t('unlock.footer', { version: auth.version }) }}
      </div>
    </div>
  </div>
</template>
