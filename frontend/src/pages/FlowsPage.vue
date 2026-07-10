<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { PhStack, PhPlus, PhTrash } from '@phosphor-icons/vue'
import { useFlowsStore } from '@/stores/flows'
import type { Flow } from '@/api/types'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { useToast } from '@/composables/useToast'
import EmptyState from '@/components/ui/EmptyState.vue'
import AppSectionLoading from '@/components/ui/SectionLoading.vue'
import AppCheckbox from '@/components/ui/Checkbox.vue'
import CronField from '@/components/forms/CronField.vue'
import { cn } from '@/lib/cn'

const { t } = useI18n()
const store = useFlowsStore()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()
const showAdd = ref(false)
const name = ref('')
const scheduleCron = ref('')
const enabled = ref(true)

onMounted(() => store.load())

function resetForm() {
  name.value = ''
  scheduleCron.value = ''
  enabled.value = true
}

async function submitAdd() {
  if (!name.value.trim()) {
    toast.error(t('flows.nameRequired'))
    return
  }
  const flow: Flow = {
    id: crypto.randomUUID(),
    name: name.value.trim(),
    schedule_cron: scheduleCron.value.trim() || undefined,
    enabled: enabled.value,
  }
  await store.add(flow)
  showAdd.value = false
  resetForm()
}

async function doDelete(id: string, flowName: string) {
  const ok = await confirmDialog({
    title: t('flows.deleteTitle'),
    message: t('flows.deleteMessage', { name: flowName }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await store.remove(id)
}
</script>

<template>
  <div class="page-shell" data-testid="page-flows">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('flows.title') }}</h1>
        <p class="page-sub">{{ t('flows.sub') }}</p>
      </div>
      <button
        class="btn-primary"
        data-testid="flows-add"
        @click="showAdd = !showAdd; if (!showAdd) resetForm()"
      >
        <PhPlus :size="16" weight="bold" /> {{ t('flows.add') }}
      </button>
    </header>

    <div v-if="showAdd" class="card mb-4 px-5 py-4" data-testid="flows-add-form">
      <h3 class="section-label">{{ t('flows.new') }}</h3>
      <form class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-end" @submit.prevent="submitAdd">
        <label class="field-label md:col-span-2">
          <span>{{ t('common.name') }}</span>
          <input v-model="name" required class="field-input" data-testid="flows-name" />
        </label>
        <div class="field-label">
          <span>{{ t('flows.schedule') }}</span>
          <CronField v-model="scheduleCron" test-id="flows-cron" allow-none />
        </div>
        <label class="flex items-center self-end pb-1">
          <AppCheckbox v-model="enabled" :label="t('common.enabled')" />
        </label>
        <p class="m-0 text-[11px] text-text-dim md:col-span-2">{{ t('flows.formHint') }}</p>
        <div class="flex justify-end gap-2 md:col-span-2">
          <button type="button" class="btn-ghost" @click="showAdd = false; resetForm()">{{ t('common.cancel') }}</button>
          <button type="submit" class="btn-primary" data-testid="flows-submit">{{ t('common.add') }}</button>
        </div>
      </form>
    </div>

    <div v-if="store.items.length > 0" class="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-2.5">
      <div v-for="f in store.items" :key="f.id" class="card p-4" :data-testid="`flow-card-${f.id}`">
        <div class="flex items-start gap-2.5 text-accent">
          <PhStack :size="20" weight="light" />
          <div>
            <div class="text-sm font-semibold text-text">{{ f.name }}</div>
            <div class="mt-0.5 text-[11px] text-text-muted">
              <template v-if="f.schedule_cron">
                {{ t('flows.cronLabel', { cron: f.schedule_cron }) }}
              </template>
              <template v-else>{{ t('flows.noSchedule') }}</template>
            </div>
          </div>
        </div>
        <div class="mt-3 flex items-center justify-between border-t border-border pt-2.5">
          <span
            :class="cn(
              'rounded px-1.5 py-px text-[11px]',
              f.enabled ? 'bg-success/15 text-success' : 'bg-surface-hover text-text-muted',
            )"
          >
            {{ f.enabled ? t('common.enabled') : t('common.disabled') }}
          </span>
          <button class="danger !p-1.5" :data-testid="`flows-delete-${f.id}`" @click="doDelete(f.id, f.name)">
            <PhTrash :size="14" weight="regular" />
          </button>
        </div>
      </div>
    </div>
    <div v-else-if="!store.loading"><EmptyState :title="t('flows.empty')" /></div>
    <div v-else><AppSectionLoading :label="t('flows.loading')" /></div>
  </div>
</template>
