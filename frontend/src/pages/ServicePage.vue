<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { PhCircleNotch, PhCheckCircle, PhXCircle, PhPlay, PhStop, PhArrowsClockwise, PhTrash, PhDownloadSimple, PhTerminal } from '@phosphor-icons/vue'
import { useServiceStore } from '@/stores/service'

const store = useServiceStore()
const showInstallConfirm = ref(false)
const showUninstallConfirm = ref(false)

onMounted(() => store.load())

function formatUptime(secs: number): string {
  if (secs <= 0) return '—'
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
    case 'scm':      return 'eventvwr.msc (Application log)'
    default:         return ''
  }
}
</script>

<template>
  <div class="service-page">
    <header class="page-header">
      <div>
        <h1>Service</h1>
        <p class="sub">Opt-in background service. The foreground mode is the default.</p>
      </div>
      <div v-if="store.status?.installed" class="status-pill" :class="{ on: store.status.running }">
        <PhCheckCircle v-if="store.status.running" :size="14" weight="fill" />
        <PhXCircle v-else-if="store.status.installed" :size="14" weight="fill" />
        <PhCircleNotch v-else :size="14" weight="fill" class="spin" />
        <span>{{ store.status.running ? 'running' : (store.status.installed ? 'stopped' : 'not installed') }}</span>
      </div>
    </header>

    <!-- State 1: Not installed -->
    <section v-if="!store.status?.installed" class="card hero">
      <div class="hero-icon"><PhDownloadSimple :size="32" weight="light" /></div>
      <h2>Install as background service</h2>
      <p>
        The service runs in the background and auto-starts on login. The web UI stays live at
        <code>127.0.0.1</code> — you can open it any time.
      </p>
      <ul class="checklist">
        <li>Main process will run in background (managed by systemd / launchd / SCM).</li>
        <li>Web UI stays accessible at loopback port.</li>
        <li>You can stop and uninstall at any time.</li>
      </ul>
      <button class="primary" @click="showInstallConfirm = true" :disabled="store.busy">
        {{ store.busy ? 'Installing…' : 'Install service' }}
      </button>
    </section>

    <!-- State 2: Installed (running or stopped) -->
    <section v-else class="card status">
      <div class="status-grid">
        <div>
          <div class="row-label">Mode</div>
          <div class="value-mono">service</div>
        </div>
        <div>
          <div class="row-label">Scope</div>
          <div class="value-mono">{{ store.status?.scope }}</div>
        </div>
        <div>
          <div class="row-label">Platform</div>
          <div class="value-mono">{{ store.status?.platform }}</div>
        </div>
        <div>
          <div class="row-label">PID</div>
          <div class="value-mono">{{ store.status?.pid || '—' }}</div>
        </div>
        <div>
          <div class="row-label">Web port</div>
          <div class="value-mono">{{ store.status?.web_port || '—' }}</div>
        </div>
        <div>
          <div class="row-label">Uptime</div>
          <div class="value-mono">{{ formatUptime(store.status?.uptime_secs || 0) }}</div>
        </div>
        <div v-if="store.status?.started_at">
          <div class="row-label">Started</div>
          <div class="value-mono small">{{ store.status.started_at }}</div>
        </div>
        <div v-if="store.status?.last_heartbeat">
          <div class="row-label">Last heartbeat</div>
          <div class="value-mono small">
            {{ store.status.last_heartbeat }}
            <span v-if="store.status.heartbeat_stale" class="stale">stale</span>
          </div>
        </div>
        <div v-if="store.status?.active_tasks?.length">
          <div class="row-label">Active tasks</div>
          <div class="value-mono small">{{ store.status.active_tasks.join(', ') }}</div>
        </div>
        <div v-if="store.status?.last_error" class="span-2">
          <div class="row-label">Last error</div>
          <div class="value-mono small danger">{{ store.status.last_error }}</div>
        </div>
      </div>

      <div class="actions">
        <button v-if="!store.status.running" class="primary" @click="store.start()" :disabled="store.busy">
          <PhPlay :size="14" weight="bold" /> Start
        </button>
        <button v-if="store.status.running" class="primary" @click="store.restart()" :disabled="store.busy">
          <PhArrowsClockwise :size="14" weight="bold" /> Restart
        </button>
        <button v-if="store.status.running" class="danger" @click="store.stop()" :disabled="store.busy">
          <PhStop :size="14" weight="bold" /> Stop
        </button>
        <button class="ghost" @click="showUninstallConfirm = true" :disabled="store.busy">
          <PhTrash :size="14" weight="regular" /> Uninstall
        </button>
      </div>

      <a v-if="logCommand()" class="logs-link" :href="logCommand().startsWith('eventvwr') ? '#' : '#'" @click.prevent>
        <PhTerminal :size="14" weight="regular" />
        View logs: <code>{{ logCommand() }}</code>
      </a>
    </section>

    <!-- Output panel (after install/uninstall) -->
    <section v-if="store.lastOutput" class="card">
      <h3>Output</h3>
      <pre class="out">{{ store.lastOutput }}</pre>
    </section>

    <!-- Install confirm dialog -->
    <div v-if="showInstallConfirm" class="modal-bg" @click.self="showInstallConfirm = false">
      <div class="modal">
        <h3>Install gn-drive as a background service?</h3>
        <p>This will register gn-drive with your init system (systemd / launchd / SCM).</p>
        <ul class="checklist">
          <li>Main process will run in the background and auto-start on login.</li>
          <li>Web UI stays live at loopback — you can open it any time.</li>
          <li>You can stop and uninstall at any time.</li>
        </ul>
        <div class="modal-actions">
          <button class="ghost" @click="showInstallConfirm = false">Cancel</button>
          <button class="primary" @click="store.install(); showInstallConfirm = false">Install</button>
        </div>
      </div>
    </div>

    <div v-if="showUninstallConfirm" class="modal-bg" @click.self="showUninstallConfirm = false">
      <div class="modal">
        <h3>Uninstall service?</h3>
        <p>This will stop the service and remove the unit file / plist / SCM entry.</p>
        <div class="modal-actions">
          <button class="ghost" @click="showUninstallConfirm = false">Cancel</button>
          <button class="danger" @click="store.uninstall(); showUninstallConfirm = false">Uninstall</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.service-page { max-width: 900px; margin: 0 auto; }
.page-header { display: flex; justify-content: space-between; align-items: flex-end; margin-bottom: 20px; gap: 16px; }
.page-header h1 { font-size: 22px; font-weight: 600; margin: 0 0 4px; }
.page-header .sub { color: var(--color-text-muted); font-size: 13px; margin: 0; }
.status-pill { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 999px; font-size: 12px; color: var(--color-text-muted); }
.status-pill.on { color: var(--color-success); border-color: color-mix(in srgb, var(--color-success) 30%, transparent); }

.card { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 10px; padding: 20px 24px; margin-bottom: 14px; }
.card.hero { text-align: center; padding: 32px 28px; }
.hero-icon { color: var(--color-accent); margin-bottom: 8px; display: flex; justify-content: center; }
.card h2 { font-size: 16px; font-weight: 600; margin: 0 0 8px; }
.card p { color: var(--color-text-muted); font-size: 13px; line-height: 1.5; margin: 0 0 12px; }
.card p code { font-family: var(--font-mono); font-size: 12px; padding: 1px 5px; background: var(--color-surface-hover); border-radius: 3px; }
.checklist { list-style: none; padding: 0; margin: 0 0 16px; text-align: left; max-width: 420px; margin-left: auto; margin-right: auto; }
.checklist li { font-size: 13px; color: var(--color-text-muted); padding: 4px 0; padding-left: 18px; position: relative; }
.checklist li::before { content: '✓'; position: absolute; left: 0; color: var(--color-success); }

.status-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; margin-bottom: 16px; }
.row-label { font-size: 10px; color: var(--color-text-dim); text-transform: uppercase; letter-spacing: 0.4px; margin-bottom: 4px; }
.value-mono { font-size: 13px; font-family: var(--font-mono); color: var(--color-text); }
.value-mono.small { font-size: 11px; }
.value-mono.danger { color: var(--color-danger); }
.span-2 { grid-column: 1 / -1; }
.stale { margin-left: 6px; padding: 1px 6px; background: color-mix(in srgb, var(--color-warning) 20%, transparent); color: var(--color-warning); border-radius: 4px; font-size: 10px; }

.actions { display: flex; gap: 8px; flex-wrap: wrap; padding-top: 12px; border-top: 1px solid var(--color-border); }
.primary, .danger, .ghost { display: inline-flex; align-items: center; gap: 6px; padding: 7px 14px; border-radius: 6px; font-size: 13px; font-weight: 500; border: 0; }
.primary { background: var(--color-accent); color: white; }
.danger { background: transparent; border: 1px solid var(--color-border); color: var(--color-text); }
.danger:hover { background: color-mix(in srgb, var(--color-danger) 12%, transparent); color: var(--color-danger); border-color: color-mix(in srgb, var(--color-danger) 30%, transparent); }
.ghost { background: transparent; border: 1px solid var(--color-border); color: var(--color-text); }
.ghost:hover { background: var(--color-surface-hover); }
button:disabled { opacity: 0.5; cursor: not-allowed; }

.logs-link { display: inline-flex; align-items: center; gap: 6px; margin-top: 12px; color: var(--color-text-muted); font-size: 11px; }
.logs-link code { font-family: var(--font-mono); padding: 1px 5px; background: var(--color-bg); border: 1px solid var(--color-border); border-radius: 3px; }

.out { font-family: var(--font-mono); font-size: 11px; color: var(--color-text-muted); background: var(--color-bg); padding: 12px; border-radius: 6px; white-space: pre-wrap; max-height: 200px; overflow: auto; }
.card h3 { font-size: 12px; font-weight: 600; text-transform: uppercase; color: var(--color-text-muted); letter-spacing: 0.5px; margin: 0 0 8px; }

.modal-bg { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.6); display: flex; align-items: center; justify-content: center; z-index: 100; }
.modal { background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 10px; padding: 24px; max-width: 460px; width: 90%; }
.modal h3 { margin: 0 0 8px; font-size: 16px; font-weight: 600; }
.modal p { color: var(--color-text-muted); font-size: 13px; margin: 0 0 12px; }
.modal-actions { display: flex; gap: 8px; justify-content: flex-end; }

.spin { animation: spin 1s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
