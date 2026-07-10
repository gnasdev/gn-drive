<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhCloud, PhPlus, PhTrash, PhCheckCircle, PhXCircle, PhSpinner } from '@phosphor-icons/vue'
import { useRemotesStore } from '@/stores/remotes'
import { useApi } from '@/composables/useApi'
import RemoteTypeSelect from '@/components/forms/RemoteTypeSelect.vue'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSectionLoading from '@/components/ui/SectionLoading.vue'
import AppAlert from '@/components/ui/Alert.vue'

const { t } = useI18n()
const store = useRemotesStore()
const api = useApi()
const { confirmDialog } = useConfirmDialog()

const showAdd = ref(false)
const newName = ref('')
const newType = ref('local')
const testResults = ref<Record<string, { ok: boolean; error?: string }>>({})

onMounted(() => store.load())

async function submitAdd() {
  if (!newName.value || !newType.value) return
  try {
    await store.add(newName.value.trim(), newType.value.trim())
    showAdd.value = false
    newName.value = ''
    newType.value = 'local'
  } catch {
    // api.error already set
  }
}

async function doTest(name: string) {
  testResults.value[name] = { ok: false }
  const r = await store.test(name)
  testResults.value[name] = r
}

async function doDelete(name: string) {
  const ok = await confirmDialog({
    title: t('remotes.deleteTitle'),
    message: t('remotes.deleteMessage', { name }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await store.remove(name)
}
</script>

<template>
  <div class="page-shell" data-testid="page-remotes">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('remotes.title') }}</h1>
        <p class="page-sub">
          <i18n-t keypath="remotes.sub" tag="span">
            <template #conf>
              <code class="rounded bg-surface-hover px-1 font-mono text-xs">rclone.conf</code>
            </template>
          </i18n-t>
        </p>
      </div>
      <button class="btn-primary" data-testid="remotes-add" @click="showAdd = !showAdd">
        <PhPlus :size="16" weight="bold" /> {{ t('remotes.add') }}
      </button>
    </header>

    <div v-if="showAdd" class="card mb-4 px-5 py-4" data-testid="remotes-add-form">
      <h3 class="section-label">{{ t('remotes.new') }}</h3>
      <form class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-end" @submit.prevent="submitAdd">
        <label class="field-label">
          <span>{{ t('common.name') }}</span>
          <input v-model="newName" placeholder="gdrive" required class="field-input" data-testid="remotes-name" />
        </label>
        <label class="field-label">
          <span>{{ t('common.type') }}</span>
          <RemoteTypeSelect v-model="newType" test-id="remotes-type" />
        </label>
        <div class="flex justify-end gap-2 md:col-span-2">
          <button type="button" class="btn-ghost" @click="showAdd = false">{{ t('common.cancel') }}</button>
          <button type="submit" class="btn-primary" :disabled="api.loading.value" data-testid="remotes-submit">
            {{ api.loading.value ? t('common.adding') : t('common.add') }}
          </button>
        </div>
      </form>
      <p class="mt-3 border-t border-border pt-3 text-[11px] text-text-dim">
        {{ t('remotes.hint') }}
      </p>
    </div>

    <AppAlert v-if="api.error.value" type="error">{{ api.error.value }}</AppAlert>

    <div v-if="store.items.length > 0 || !store.loading" class="table-wrap">
      <table class="data-table">
        <thead>
          <tr>
            <th>{{ t('remotes.colName') }}</th>
            <th>{{ t('remotes.colType') }}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in store.items" :key="r.name">
            <td>
              <div class="flex items-center gap-2 text-text-muted">
                <PhCloud :size="16" weight="regular" />
                <span class="font-mono text-text">{{ r.name }}</span>
              </div>
            </td>
            <td><span class="badge">{{ r.type || 'unknown' }}</span></td>
            <td class="whitespace-nowrap text-right">
              <button
                class="btn-ghost !px-2 !py-1"
                :title="t('remotes.testTitle', { name: r.name })"
                :data-testid="`remotes-test-${r.name}`"
                @click="doTest(r.name)"
              >
                <template v-if="testResults[r.name]?.ok === true">
                  <PhCheckCircle :size="16" weight="fill" class="text-success" />
                </template>
                <template v-else-if="testResults[r.name]?.ok === false && testResults[r.name]?.error">
                  <PhXCircle :size="16" weight="fill" class="text-danger" />
                </template>
                <template v-else-if="store.loading">
                  <PhSpinner :size="16" class="animate-spin" />
                </template>
                <template v-else>{{ t('common.test') }}</template>
              </button>
              <button
                class="danger ml-1 !p-1.5"
                :title="t('remotes.deleteTitleBtn', { name: r.name })"
                :data-testid="`remotes-delete-${r.name}`"
                @click="doDelete(r.name)"
              >
                <PhTrash :size="14" weight="regular" />
              </button>
            </td>
          </tr>
          <tr v-if="store.items.length === 0 && !store.loading">
            <td colspan="3">
              <EmptyState :title="t('remotes.empty')" :description="t('remotes.emptyDesc')" />
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else><AppSectionLoading /></div>
  </div>
</template>
