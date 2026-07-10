<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  PhCircleNotch,
  PhCheckCircle,
  PhXCircle,
  PhPlay,
  PhStop,
  PhArrowsClockwise,
  PhTrash,
  PhDownloadSimple,
  PhTerminal,
} from '@phosphor-icons/vue'
import { useServiceStore } from '@/stores/service'
import AppDialog from '@/components/ui/Dialog.vue'
import { cn } from '@/lib/cn'

const { t } = useI18n()
const store = useServiceStore()
const showInstallConfirm = ref(false)
const showUninstallConfirm = ref(false)

onMounted(() => store.load())

function formatUptime(secs: number): string {
  if (secs <= 0) return t('common.empty')
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${secs % 60}s`
  return `${secs}s`
}

function logCommand(): string {
  switch (store.status?.platform) {
    case 'systemd': return 'journalctl --user -u gn-drive -f'
    case 'launchd': return 'log show --predicate \'process == "gn-drive"\' --follow'
    case 'scm': return 'eventvwr.msc (Application log)'
    default: return ''
  }
}

function statusLabel(): string {
  if (!store.status) return t('service.notInstalled')
  if (store.status.running) return t('service.running')
  if (store.status.installed) return t('service.stopped')
  return t('service.notInstalled')
}
</script>

<template>
  <div class="mx-auto max-w-[900px]" data-testid="page-service">
    <header class="mb-5 flex items-end justify-between gap-4">
      <div>
        <h1 class="page-title">{{ t('service.title') }}</h1>
        <p class="page-sub">{{ t('service.sub') }}</p>
      </div>
      <div
        v-if="store.status?.installed"
        :class="cn(
          'inline-flex items-center gap-1.5 rounded-full border border-border bg-surface px-2.5 py-1 text-xs text-text-muted',
          store.status.running && 'border-success/30 text-success',
        )"
      >
        <PhCheckCircle v-if="store.status.running" :size="14" weight="fill" />
        <PhXCircle v-else-if="store.status.installed" :size="14" weight="fill" />
        <PhCircleNotch v-else :size="14" weight="fill" class="animate-spin" />
        <span>{{ statusLabel() }}</span>
      </div>
    </header>

    <section v-if="!store.status?.installed" class="card mb-3.5 px-7 py-8 text-center">
      <div class="mb-2 flex justify-center text-accent">
        <PhDownloadSimple :size="32" weight="light" />
      </div>
      <h2 class="mb-2 text-base font-semibold">{{ t('service.installTitle') }}</h2>
      <p class="mb-3 text-[13px] leading-relaxed text-text-muted">
        <i18n-t keypath="service.installBody" tag="span">
          <template #host>
            <code class="rounded bg-surface-hover px-1 font-mono text-xs">127.0.0.1</code>
          </template>
        </i18n-t>
      </p>
      <ul class="mx-auto mb-4 max-w-[420px] list-none p-0 text-left">
        <li class="relative py-1 pl-[18px] text-[13px] text-text-muted before:absolute before:left-0 before:text-success before:content-['✓']">
          {{ t('service.check1') }}
        </li>
        <li class="relative py-1 pl-[18px] text-[13px] text-text-muted before:absolute before:left-0 before:text-success before:content-['✓']">
          {{ t('service.check2') }}
        </li>
        <li class="relative py-1 pl-[18px] text-[13px] text-text-muted before:absolute before:left-0 before:text-success before:content-['✓']">
          {{ t('service.check3') }}
        </li>
      </ul>
      <button class="btn-primary" :disabled="store.busy" @click="showInstallConfirm = true">
        {{ store.busy ? t('service.installing') : t('service.install') }}
      </button>
    </section>

    <section v-else class="card mb-3.5 px-6 py-5">
      <div class="mb-4 grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-3">
        <div>
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.mode') }}</div>
          <div class="font-mono text-[13px]">service</div>
        </div>
        <div>
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.scope') }}</div>
          <div class="font-mono text-[13px]">{{ store.status?.scope }}</div>
        </div>
        <div>
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.platform') }}</div>
          <div class="font-mono text-[13px]">{{ store.status?.platform }}</div>
        </div>
        <div>
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.pid') }}</div>
          <div class="font-mono text-[13px]">{{ store.status?.pid || t('common.empty') }}</div>
        </div>
        <div>
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.webPort') }}</div>
          <div class="font-mono text-[13px]">{{ store.status?.web_port || t('common.empty') }}</div>
        </div>
        <div>
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.uptime') }}</div>
          <div class="font-mono text-[13px]">{{ formatUptime(store.status?.uptime_secs || 0) }}</div>
        </div>
        <div v-if="store.status?.started_at">
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.started') }}</div>
          <div class="font-mono text-[11px]">{{ store.status.started_at }}</div>
        </div>
        <div v-if="store.status?.last_heartbeat">
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.lastHeartbeat') }}</div>
          <div class="font-mono text-[11px]">
            {{ store.status.last_heartbeat }}
            <span
              v-if="store.status.heartbeat_stale"
              class="ml-1.5 rounded bg-warning/20 px-1.5 py-px text-[10px] text-warning"
            >
              {{ t('service.stale') }}
            </span>
          </div>
        </div>
        <div v-if="store.status?.active_tasks?.length">
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.activeTasks') }}</div>
          <div class="font-mono text-[11px]">{{ store.status.active_tasks.join(', ') }}</div>
        </div>
        <div v-if="store.status?.last_error" class="col-span-full">
          <div class="mb-1 text-[10px] uppercase tracking-wide text-text-dim">{{ t('service.lastError') }}</div>
          <div class="font-mono text-[11px] text-danger">{{ store.status.last_error }}</div>
        </div>
      </div>

      <div class="flex flex-wrap gap-2 border-t border-border pt-3">
        <button v-if="!store.status.running" class="btn-primary" :disabled="store.busy" @click="store.start()">
          <PhPlay :size="14" weight="bold" /> {{ t('service.start') }}
        </button>
        <button v-if="store.status.running" class="btn-primary" :disabled="store.busy" @click="store.restart()">
          <PhArrowsClockwise :size="14" weight="bold" /> {{ t('service.restart') }}
        </button>
        <button v-if="store.status.running" class="danger !px-3.5 !py-1.5" :disabled="store.busy" @click="store.stop()">
          <PhStop :size="14" weight="bold" /> {{ t('service.stop') }}
        </button>
        <button class="btn-ghost" :disabled="store.busy" @click="showUninstallConfirm = true">
          <PhTrash :size="14" weight="regular" /> {{ t('service.uninstall') }}
        </button>
      </div>

      <a
        v-if="logCommand()"
        class="mt-3 inline-flex items-center gap-1.5 text-[11px] text-text-muted"
        href="#"
        @click.prevent
      >
        <PhTerminal :size="14" weight="regular" />
        {{ t('service.viewLogs') }} <code class="rounded border border-border bg-bg px-1 font-mono">{{ logCommand() }}</code>
      </a>
    </section>

    <section v-if="store.lastOutput" class="card mb-3.5 px-6 py-5">
      <h3 class="section-label">{{ t('service.output') }}</h3>
      <pre class="max-h-[200px] overflow-auto whitespace-pre-wrap rounded-md bg-bg p-3 font-mono text-[11px] text-text-muted">{{ store.lastOutput }}</pre>
    </section>

    <AppDialog v-model="showInstallConfirm" :title="t('service.installDialogTitle')" size="sm">
      <p class="mb-3 text-[13px] text-text-muted">{{ t('service.installDialogBody') }}</p>
      <ul class="mb-4 list-none p-0">
        <li class="relative py-1 pl-[18px] text-[13px] text-text-muted before:absolute before:left-0 before:text-success before:content-['✓']">
          {{ t('service.installDlg1') }}
        </li>
        <li class="relative py-1 pl-[18px] text-[13px] text-text-muted before:absolute before:left-0 before:text-success before:content-['✓']">
          {{ t('service.installDlg2') }}
        </li>
        <li class="relative py-1 pl-[18px] text-[13px] text-text-muted before:absolute before:left-0 before:text-success before:content-['✓']">
          {{ t('service.installDlg3') }}
        </li>
      </ul>
      <div class="flex justify-end gap-2">
        <button class="btn-ghost" @click="showInstallConfirm = false">{{ t('common.cancel') }}</button>
        <button class="btn-primary" @click="store.install(); showInstallConfirm = false">{{ t('service.installShort') }}</button>
      </div>
    </AppDialog>

    <AppDialog v-model="showUninstallConfirm" :title="t('service.uninstallTitle')" size="sm">
      <p class="mb-4 text-[13px] text-text-muted">{{ t('service.uninstallBody') }}</p>
      <div class="flex justify-end gap-2">
        <button class="btn-ghost" @click="showUninstallConfirm = false">{{ t('common.cancel') }}</button>
        <button class="danger !px-3.5 !py-1.5" @click="store.uninstall(); showUninstallConfirm = false">
          {{ t('service.uninstall') }}
        </button>
      </div>
    </AppDialog>
  </div>
</template>
