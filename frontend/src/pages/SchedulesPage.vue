<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhCalendar, PhPlus, PhTrash, PhPause, PhPlay } from '@phosphor-icons/vue'
import { useSchedulesStore } from '@/stores/schedules'
import { useProfilesStore } from '@/stores/profiles'
import { useApi } from '@/composables/useApi'
import type { Schedule } from '@/api/types'
import { SYNC_ACTIONS } from '@/constants/forms'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { useToast } from '@/composables/useToast'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSectionLoading from '@/components/ui/SectionLoading.vue'
import AppAlert from '@/components/ui/Alert.vue'

const { t } = useI18n()
const store = useSchedulesStore()
const profiles = useProfilesStore()
const api = useApi()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()

const showAdd = ref(false)
const draft = ref<Schedule>({ id: '', profile_name: '', action: 'pull', cron: '0 * * * *', enabled: true })

onMounted(async () => {
  await Promise.all([store.load(), profiles.load()])
})

async function submitAdd() {
  if (!draft.value.profile_name || !draft.value.cron) {
    toast.error(t('schedules.required'))
    return
  }
  await store.add({ ...draft.value, id: crypto.randomUUID() })
  showAdd.value = false
  draft.value = { id: '', profile_name: '', action: 'pull', cron: '0 * * * *', enabled: true }
}

async function doDelete(id: string) {
  const ok = await confirmDialog({
    title: t('schedules.deleteTitle'),
    message: t('schedules.deleteMessage'),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await store.remove(id)
}
</script>

<template>
  <div class="page-shell-wide" data-testid="page-schedules">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('schedules.title') }}</h1>
        <p class="page-sub">{{ t('schedules.sub') }}</p>
      </div>
      <button class="btn-primary" data-testid="schedules-add" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> {{ t('schedules.add') }}
      </button>
    </header>

    <div v-if="showAdd" class="card mb-4 px-5 py-4" data-testid="schedules-add-form">
      <h3 class="section-label">{{ t('schedules.new') }}</h3>
      <form class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-end" @submit.prevent="submitAdd">
        <label class="field-label md:col-span-2">
          <span>{{ t('common.profile') }}</span>
          <select v-model="draft.profile_name" required class="field-input" data-testid="schedules-profile">
            <option value="" disabled>{{ t('common.selectProfile') }}</option>
            <option v-for="p in profiles.items" :key="p.name" :value="p.name">{{ p.name }}</option>
          </select>
        </label>
        <label class="field-label">
          <span>{{ t('common.action') }}</span>
          <select v-model="draft.action" class="field-input" data-testid="schedules-action">
            <option v-for="a in SYNC_ACTIONS" :key="a" :value="a">{{ a }}</option>
          </select>
        </label>
        <label class="field-label">
          <span>{{ t('schedules.cron') }}</span>
          <input v-model="draft.cron" placeholder="0 * * * *" required class="field-input" data-testid="schedules-cron" />
        </label>
        <p class="md:col-span-2 m-0 text-[11px] text-text-dim">
          <i18n-t keypath="schedules.cronHint" tag="span">
            <template #ex>
              <code class="font-mono">0 * * * *</code>
            </template>
          </i18n-t>
        </p>
        <div class="flex justify-end gap-2 md:col-span-2">
          <button type="button" class="btn-ghost" @click="showAdd = false">{{ t('common.cancel') }}</button>
          <button type="submit" class="btn-primary" :disabled="api.loading.value" data-testid="schedules-submit">
            {{ api.loading.value ? t('common.adding') : t('common.add') }}
          </button>
        </div>
      </form>
    </div>

    <AppAlert v-if="api.error.value" type="error">{{ api.error.value }}</AppAlert>

    <div v-if="store.items.length > 0 || !store.loading" class="table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>{{ t('schedules.colProfile') }}</th>
            <th>{{ t('schedules.colAction') }}</th>
            <th>{{ t('schedules.colCron') }}</th>
            <th>{{ t('schedules.colLast') }}</th>
            <th>{{ t('schedules.colNext') }}</th>
            <th>{{ t('schedules.colEnabled') }}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="s in store.items"
            :key="s.id"
            :data-testid="`schedule-row-${s.id}`"
          >
            <td>
              <div class="flex items-center gap-1.5 text-text-muted">
                <PhCalendar :size="14" weight="regular" />
                <span class="font-mono text-text">{{ s.profile_name }}</span>
              </div>
            </td>
            <td><span class="badge">{{ s.action }}</span></td>
            <td class="font-mono text-[11px] text-text">{{ s.cron }}</td>
            <td class="text-[11px] text-text-muted">{{ s.last_run || t('common.empty') }}</td>
            <td class="text-[11px] text-text-muted">{{ s.next_run || t('common.empty') }}</td>
            <td>
              <button
                class="toggle"
                :class="{ on: s.enabled }"
                :data-testid="`schedules-toggle-${s.id}`"
                @click="s.enabled ? store.disable(s.id) : store.enable(s.id)"
              >
                <PhPause v-if="s.enabled" :size="14" weight="bold" />
                <PhPlay v-else :size="14" weight="bold" />
                <span>{{ s.enabled ? t('common.enabled') : t('common.disabled') }}</span>
              </button>
            </td>
            <td class="text-right">
              <button class="danger !p-1.5" :data-testid="`schedules-delete-${s.id}`" @click="doDelete(s.id)">
                <PhTrash :size="14" weight="regular" />
              </button>
            </td>
          </tr>
          <tr v-if="store.items.length === 0 && !store.loading">
            <td colspan="7"><EmptyState :title="t('schedules.empty')" /></td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else><AppSectionLoading /></div>
  </div>
</template>
