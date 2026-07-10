<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhKey, PhPlus, PhTrash, PhArrowRight, PhPencilSimple } from '@phosphor-icons/vue'
import { useProfilesStore } from '@/stores/profiles'
import { useRemotesStore } from '@/stores/remotes'
import type { Profile } from '@/api/types'
import { SYNC_ACTIONS } from '@/constants/forms'
import RemotePathField from '@/components/forms/RemotePathField.vue'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { useToast } from '@/composables/useToast'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSectionLoading from '@/components/ui/SectionLoading.vue'
import AppAlert from '@/components/ui/Alert.vue'

const { t } = useI18n()
const store = useProfilesStore()
const remotes = useRemotesStore()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()

const formOpen = ref(false)
const formMode = ref<'create' | 'edit'>('create')
const draft = ref<Profile>(emptyDraft())

function emptyDraft(): Profile {
  return {
    name: '',
    from: '',
    to: '',
    direction: 'push',
    parallel: 4,
    bandwidth: 0,
    dry_run: false,
  }
}

onMounted(async () => {
  await Promise.all([store.load(), remotes.load()])
})

function openCreate() {
  formMode.value = 'create'
  draft.value = emptyDraft()
  formOpen.value = true
}

function openEdit(p: Profile) {
  formMode.value = 'edit'
  draft.value = {
    ...p,
    direction: p.direction || 'push',
    parallel: p.parallel || 4,
    bandwidth: p.bandwidth ?? 0,
    dry_run: !!p.dry_run,
  }
  formOpen.value = true
}

function closeForm() {
  formOpen.value = false
  draft.value = emptyDraft()
}

async function submitForm() {
  if (!draft.value.name || !draft.value.from || !draft.value.to) {
    toast.error(t('profiles.required'))
    return
  }
  try {
    if (formMode.value === 'create') {
      await store.add({ ...draft.value })
      toast.success(t('profiles.added'))
    } else {
      await store.update({ ...draft.value })
      toast.success(t('profiles.updated'))
    }
    closeForm()
  } catch {
    // api.error already set
  }
}

async function doDelete(name: string) {
  const ok = await confirmDialog({
    title: t('profiles.deleteTitle'),
    message: t('profiles.deleteMessage', { name }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await store.remove(name)
}
</script>

<template>
  <div class="page-shell-wide" data-testid="page-profiles">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('profiles.title') }}</h1>
        <p class="page-sub">
          <i18n-t keypath="profiles.sub" tag="span">
            <template #db>
              <code class="rounded bg-surface-hover px-1 font-mono text-xs">gn-drive.db</code>
            </template>
          </i18n-t>
        </p>
      </div>
      <button class="btn-primary" data-testid="profiles-add" @click="formOpen ? closeForm() : openCreate()">
        <PhPlus :size="16" weight="bold" />
        {{ formOpen && formMode === 'create' ? t('profiles.close') : t('profiles.add') }}
      </button>
    </header>

    <div
      v-if="formOpen"
      class="card mb-4 px-5 py-4"
      :data-testid="formMode === 'create' ? 'profiles-add-form' : 'profiles-edit-form'"
    >
      <h3 class="section-label">{{ formMode === 'create' ? t('profiles.new') : t('profiles.edit') }}</h3>
      <form class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-end" @submit.prevent="submitForm">
        <label class="field-label">
          <span>{{ t('common.name') }}</span>
          <input
            v-model="draft.name"
            required
            class="field-input"
            data-testid="profiles-name"
            :readonly="formMode === 'edit'"
            :class="{ 'cursor-not-allowed opacity-70': formMode === 'edit' }"
          />
        </label>
        <label class="field-label">
          <span>{{ t('profiles.direction') }}</span>
          <select v-model="draft.direction" class="field-input" data-testid="profiles-direction">
            <option v-for="a in SYNC_ACTIONS" :key="a" :value="a">{{ a }}</option>
          </select>
        </label>

        <div class="md:col-span-2">
          <RemotePathField
            v-model="draft.from"
            :remotes="remotes.items"
            test-id="profiles-from"
            :label="t('profiles.fromLabel')"
            required
          />
        </div>
        <div class="md:col-span-2">
          <RemotePathField
            v-model="draft.to"
            :remotes="remotes.items"
            test-id="profiles-to"
            :label="t('profiles.toLabel')"
            required
          />
        </div>

        <label class="field-label">
          <span>{{ t('profiles.parallel') }}</span>
          <input v-model.number="draft.parallel" type="number" min="1" max="64" class="field-input" data-testid="profiles-parallel" />
        </label>
        <label class="field-label">
          <span>{{ t('profiles.bandwidth') }}</span>
          <input v-model.number="draft.bandwidth" type="number" min="0" class="field-input" data-testid="profiles-bandwidth" />
        </label>
        <label class="flex flex-row items-center gap-2 md:col-span-2">
          <input v-model="draft.dry_run" type="checkbox" class="accent-accent" data-testid="profiles-dry-run" />
          <span class="text-[13px]">{{ t('profiles.dryRun') }}</span>
        </label>
        <div class="flex justify-end gap-2 md:col-span-2">
          <button type="button" class="btn-ghost" @click="closeForm">{{ t('common.cancel') }}</button>
          <button type="submit" class="btn-primary" :disabled="store.loading" data-testid="profiles-submit">
            {{ store.loading ? t('common.saving') : formMode === 'create' ? t('common.add') : t('common.save') }}
          </button>
        </div>
      </form>
    </div>

    <AppAlert v-if="store.error" type="error">{{ store.error }}</AppAlert>

    <div v-if="store.items.length > 0 || !store.loading" class="table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>{{ t('profiles.colName') }}</th>
            <th>{{ t('profiles.colFrom') }}</th>
            <th></th>
            <th>{{ t('profiles.colTo') }}</th>
            <th>{{ t('profiles.colDir') }}</th>
            <th>{{ t('profiles.colPar') }}</th>
            <th>{{ t('profiles.colBw') }}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in store.items" :key="p.name">
            <td>
              <div class="flex items-center gap-1.5 text-text-muted">
                <PhKey :size="14" weight="regular" />
                <span class="font-mono text-text">{{ p.name }}</span>
              </div>
            </td>
            <td class="max-w-[200px] truncate font-mono text-[11px] text-text" :title="p.from">{{ p.from }}</td>
            <td class="text-center text-text-dim"><PhArrowRight :size="12" weight="bold" /></td>
            <td class="max-w-[200px] truncate font-mono text-[11px] text-text" :title="p.to">{{ p.to }}</td>
            <td><span class="badge">{{ p.direction || '-' }}</span></td>
            <td class="text-right font-mono text-text-muted">{{ p.parallel }}</td>
            <td class="text-right font-mono text-text-muted">{{ p.bandwidth > 0 ? p.bandwidth + 'M' : '∞' }}</td>
            <td class="whitespace-nowrap text-right">
              <button class="btn-ghost !p-1.5" :data-testid="`profiles-edit-${p.name}`" :title="t('common.edit')" @click="openEdit(p)">
                <PhPencilSimple :size="14" weight="regular" />
              </button>
              <button class="danger ml-1 !p-1.5" :data-testid="`profiles-delete-${p.name}`" @click="doDelete(p.name)">
                <PhTrash :size="14" weight="regular" />
              </button>
            </td>
          </tr>
          <tr v-if="store.items.length === 0 && !store.loading">
            <td colspan="8"><EmptyState :title="t('profiles.empty')" /></td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else><AppSectionLoading :label="t('profiles.loading')" /></div>
  </div>
</template>
