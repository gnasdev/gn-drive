<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useToast } from '@gnas/ui-shared'
import AppAlert from '@gnas/ui-shared/components/AppAlert.vue'

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
    return toast.error('Passwords do not match.')
  }
  if (password.value.length < 4) {
    return toast.error('Password must be at least 4 characters.')
  }
  try {
    if (mode.value === 'setup') {
      await auth.doSetup(password.value)
    } else {
      await auth.unlock(password.value)
    }
    router.push({ name: 'dashboard' })
  } catch (e) {
    // error already in store
  }
}
</script>

<template>
  <div class="unlock-page" data-testid="page-unlock">
    <div class="card">
      <div class="brand">
        <div class="mark">GN</div>
        <div class="name">GN Drive</div>
      </div>

      <h1 class="title" data-testid="unlock-title">
        {{ mode === 'setup' ? 'Set up master password' : 'Unlock' }}
      </h1>
      <p class="sub">
        <template v-if="mode === 'setup'">
          Choose a password. It will encrypt <code>gn-drive.db</code> and
          <code>rclone.conf</code> at rest using Argon2id + AES-256-GCM.
        </template>
        <template v-else>
          Enter your master password to decrypt and access gn-drive.
        </template>
      </p>

      <form @submit.prevent="submit" class="form" data-testid="unlock-form">
        <label class="field">
          <span>Password</span>
          <input
            v-model="password"
            type="password"
            autofocus
            autocomplete="current-password"
            :disabled="auth.busy"
            data-testid="unlock-password"
          />
        </label>

        <label v-if="mode === 'setup'" class="field">
          <span>Confirm</span>
          <input
            v-model="confirm"
            type="password"
            autocomplete="new-password"
            :disabled="auth.busy"
            data-testid="unlock-confirm"
          />
        </label>

        <AppAlert v-if="auth.error" type="error" data-testid="unlock-error">{{ auth.error }}</AppAlert>

        <button type="submit" class="primary" :disabled="auth.busy || !password" data-testid="unlock-submit">
          {{ auth.busy ? '…' : (mode === 'setup' ? 'Set up & unlock' : 'Unlock') }}
        </button>
      </form>

      <div class="footer">
        v{{ auth.version }} · loopback only
      </div>
    </div>
  </div>
</template>

<style scoped>
.unlock-page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-bg);
  padding: 24px;
}
.card {
  width: 100%;
  max-width: 380px;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 10px;
  padding: 32px 28px;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 24px;
}
.mark {
  width: 32px; height: 32px;
  background: var(--color-accent);
  color: white;
  border-radius: 6px;
  display: flex; align-items: center; justify-content: center;
  font-weight: 700;
  font-size: 13px;
}
.name { font-weight: 600; }
.title { font-size: 18px; font-weight: 600; margin: 0 0 8px; }
.sub {
  color: var(--color-text-muted);
  font-size: 13px;
  line-height: 1.5;
  margin: 0 0 20px;
}
.sub code {
  font-family: var(--font-mono);
  font-size: 12px;
  padding: 1px 4px;
  background: var(--color-surface-hover);
  border-radius: 3px;
}
.form { display: flex; flex-direction: column; gap: 14px; }
.field { display: flex; flex-direction: column; gap: 4px; }
.field span {
  font-size: 12px;
  color: var(--color-text-muted);
  font-weight: 500;
}
.field input {
  padding: 8px 10px;
  background: var(--color-bg);
  border: 1px solid var(--color-border);
  border-radius: 6px;
  color: var(--color-text);
  font-family: inherit;
  font-size: 13px;
}
.field input:focus {
  outline: none;
  border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent);
}
.field input:disabled { opacity: 0.6; }
.primary {
  padding: 9px 14px;
  background: var(--color-accent);
  color: white;
  border: 0;
  border-radius: 6px;
  font-weight: 600;
  font-size: 13px;
  margin-top: 4px;
}
.primary:disabled { opacity: 0.5; cursor: not-allowed; }
.error {
  font-size: 12px;
  color: var(--color-danger);
  background: color-mix(in srgb, var(--color-danger) 12%, transparent);
  padding: 8px 10px;
  border-radius: 6px;
}
.footer {
  margin-top: 24px;
  text-align: center;
  font-size: 11px;
  color: var(--color-text-dim);
  font-family: var(--font-mono);
}
</style>
