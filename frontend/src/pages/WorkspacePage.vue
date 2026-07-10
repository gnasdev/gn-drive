<script setup lang="ts">
/**
 * Single-page workspace ported from desktop Angular v0.4.x:
 * topbar + scrollable neo cards where remotes, sync operations (profiles),
 * boards, and flows live together — no multi-page sidebar.
 */
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref } from 'vue'

/** Required for <KeepAlive include="WorkspacePage"> in App.vue */
defineOptions({ name: 'WorkspacePage' })
import {
  errorsByField,
  isProfileDraftValid,
  validateProfileDraft,
} from '@/lib/profileValidation'
import { useI18n } from 'vue-i18n'
import {
  PhPlus,
  PhPlay,
  PhStop,
  PhTrash,
  PhPencilSimple,
  PhCloud,
  PhArrowRight,
  PhArrowDown,
  PhStack,
  PhSquaresFour,
  PhCheckCircle,
  PhXCircle,
  PhSpinner,
  PhClock,
} from '@phosphor-icons/vue'
import { useProfilesStore } from '@/stores/profiles'
import { useRemotesStore } from '@/stores/remotes'
import { useBoardsStore } from '@/stores/boards'
import { useFlowsStore } from '@/stores/flows'
import { useOperationsStore } from '@/stores/operations'
import type { Profile, Board, BoardEdge, BoardNode, Flow } from '@/api/types'
import {
  BOARD_EDGE_ACTIONS,
  composedPathToBoardNode,
} from '@/constants/forms'
import RemotePathField from '@/components/forms/RemotePathField.vue'
import RemoteTypeSelect from '@/components/forms/RemoteTypeSelect.vue'
import DirectionField from '@/components/forms/DirectionField.vue'
import CronField from '@/components/forms/CronField.vue'
import AppCheckbox from '@/components/ui/Checkbox.vue'
import AppAlert from '@/components/ui/Alert.vue'
import { useConfirmDialog } from '@/composables/useConfirmDialog'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const profiles = useProfilesStore()
const remotes = useRemotesStore()
const boards = useBoardsStore()
const flows = useFlowsStore()
const ops = useOperationsStore()
const { confirmDialog } = useConfirmDialog()
const toast = useToast()

const loading = ref(true)
const pollTimer = ref<ReturnType<typeof setInterval> | null>(null)

// --- Remotes form ---
const showRemoteForm = ref(false)
const remoteName = ref('')
const remoteType = ref('local')
/** Per-remote test UI state — never seed with {ok:false} (that looks like a failed test). */
type RemoteTestState = { status: 'loading' } | { status: 'ok' } | { status: 'error'; error?: string }
const remoteTest = ref<Record<string, RemoteTestState>>({})

// --- Profile (operation) form ---
const showProfileForm = ref(false)
const profileMode = ref<'create' | 'edit'>('create')
const profileDraft = ref<Profile>(emptyProfile())
const expandedProfile = ref<string | null>(null)

// --- Board form ---
const showBoardForm = ref(false)
const boardName = ref('')
const boardSource = ref('')
const boardTarget = ref('')
const boardAction = ref('copy')

// --- Flow form ---
const showFlowForm = ref(false)
const flowName = ref('')
const flowCron = ref('')
const flowEnabled = ref(true)

function emptyProfile(): Profile {
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

const activeTasks = computed(() =>
  (ops.tasks ?? []).filter((x) => x.status === 'running' || x.status === 'pending'),
)

const dataLoaded = ref(false)

function startTaskPoll() {
  if (pollTimer.value) return
  pollTimer.value = setInterval(() => {
    void ops.loadTasks()
  }, 4000)
}

function stopTaskPoll() {
  if (pollTimer.value) {
    clearInterval(pollTimer.value)
    pollTimer.value = null
  }
}

async function loadWorkspaceData() {
  await Promise.all([
    profiles.load(),
    remotes.load(),
    boards.load(),
    flows.load(),
    ops.loadTasks(),
  ])
}

onMounted(async () => {
  try {
    await loadWorkspaceData()
    dataLoaded.value = true
  } finally {
    loading.value = false
  }
  startTaskPoll()
})

// KeepAlive: on first mount Vue also runs onActivated — skip double-fetch.
// Later returns from Settings refresh lists without wiping local form state.
onActivated(() => {
  startTaskPoll()
  if (dataLoaded.value) {
    void loadWorkspaceData()
  }
})

onDeactivated(() => {
  stopTaskPoll()
})

onUnmounted(() => {
  stopTaskPoll()
})

// —— Remotes ——
async function submitRemote() {
  if (!remoteName.value.trim() || !remoteType.value) return
  try {
    await remotes.add(remoteName.value.trim(), remoteType.value.trim())
    showRemoteForm.value = false
    remoteName.value = ''
    remoteType.value = 'local'
    toast.success(t('workspace.remoteAdded'))
  } catch {
    /* store error */
  }
}

async function testRemote(name: string) {
  remoteTest.value = { ...remoteTest.value, [name]: { status: 'loading' } }
  const r = await remotes.test(name)
  remoteTest.value = {
    ...remoteTest.value,
    [name]: r.ok ? { status: 'ok' } : { status: 'error', error: r.error },
  }
}

function remoteTestTitle(name: string): string {
  const st = remoteTest.value[name]
  if (st?.status === 'error' && st.error) return st.error
  return t('remotes.testTitle', { name })
}

async function deleteRemote(name: string) {
  const ok = await confirmDialog({
    title: t('remotes.deleteTitle'),
    message: t('remotes.deleteMessage', { name }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await remotes.remove(name)
}

// —— Profiles as operations ——
type ProfileFieldKey = 'name' | 'from' | 'to' | 'parallel' | 'bandwidth' | 'direction'
const profileTouched = ref<Partial<Record<ProfileFieldKey, boolean>>>({})

function resetProfileTouched() {
  profileTouched.value = {}
}

function touchProfileField(field: ProfileFieldKey) {
  profileTouched.value = { ...profileTouched.value, [field]: true }
}

function openCreateProfile() {
  profileMode.value = 'create'
  profileDraft.value = emptyProfile()
  resetProfileTouched()
  showProfileForm.value = true
}

function openEditProfile(p: Profile) {
  profileMode.value = 'edit'
  profileDraft.value = {
    ...p,
    direction: p.direction || 'push',
    parallel: p.parallel || 4,
    bandwidth: p.bandwidth ?? 0,
    dry_run: !!p.dry_run,
  }
  resetProfileTouched()
  showProfileForm.value = true
}

const profileErrors = computed(() => validateProfileDraft(profileDraft.value))
const profileFieldErrors = computed(() => errorsByField(profileErrors.value))
const profileFormValid = computed(() => isProfileDraftValid(profileDraft.value))
function fieldError(field: ProfileFieldKey): string | null {
  if (!profileTouched.value[field]) return null
  const e = profileFieldErrors.value[field]
  if (!e) return null
  return t(`profiles.validation.${e.messageKey}`, e.params ?? {})
}

async function submitProfile() {
  if (!profileFormValid.value) {
    // Reveal all field errors if user somehow submits invalid form.
    for (const f of ['name', 'from', 'to', 'parallel', 'bandwidth', 'direction'] as ProfileFieldKey[]) {
      profileTouched.value[f] = true
    }
    profileTouched.value = { ...profileTouched.value }
    return
  }
  try {
    if (profileMode.value === 'create') {
      await profiles.add({ ...profileDraft.value })
      toast.success(t('profiles.added'))
    } else {
      await profiles.update({ ...profileDraft.value })
      toast.success(t('profiles.updated'))
    }
    showProfileForm.value = false
    profileDraft.value = emptyProfile()
    resetProfileTouched()
  } catch {
    /* store error — shown via profiles.error in form */
  }
}

async function deleteProfile(name: string) {
  const ok = await confirmDialog({
    title: t('profiles.deleteTitle'),
    message: t('profiles.deleteMessage', { name }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await profiles.remove(name)
}

async function runProfile(p: Profile) {
  const action = p.direction || 'push'
  const id = await ops.startSync(action, p.name)
  if (id) {
    toast.success(t('workspace.syncStarted', { name: p.name }))
    await ops.loadTasks()
  } else if (ops.error) {
    toast.error(String(ops.error))
  }
}

// —— Boards ——
async function submitBoard() {
  if (!boardName.value.trim()) {
    toast.error(t('boards.nameRequired'))
    return
  }
  if (!boardSource.value.trim() || !boardTarget.value.trim()) {
    toast.error(t('boards.pathsRequired'))
    return
  }
  const src = composedPathToBoardNode(boardSource.value)
  const dst = composedPathToBoardNode(boardTarget.value)
  const id = crypto.randomUUID()
  const n1: BoardNode = {
    id: crypto.randomUUID(),
    remote_name: src.remote_name,
    path: src.path || '/',
    label: 'source',
    x: 0,
    y: 0,
  }
  const n2: BoardNode = {
    id: crypto.randomUUID(),
    remote_name: dst.remote_name,
    path: dst.path || '/',
    label: 'target',
    x: 200,
    y: 0,
  }
  const edges: BoardEdge[] = [
    {
      id: crypto.randomUUID(),
      source_id: n1.id,
      target_id: n2.id,
      action: boardAction.value || 'copy',
    },
  ]
  const board: Board = {
    id,
    name: boardName.value.trim(),
    created_at: '',
    updated_at: '',
    nodes: [n1, n2],
    edges,
  }
  await boards.add(board)
  showBoardForm.value = false
  boardName.value = ''
  boardSource.value = ''
  boardTarget.value = ''
  boardAction.value = 'copy'
  toast.success(t('workspace.boardAdded'))
}

async function deleteBoard(id: string, name: string) {
  const ok = await confirmDialog({
    title: t('boards.deleteTitle'),
    message: t('boards.deleteMessage', { name }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await boards.remove(id)
}

async function runBoard(id: string) {
  try {
    await boards.execute(id, true)
    toast.success(t('boards.execStarted'))
    await ops.loadTasks()
  } catch (e: any) {
    toast.error(e?.message ?? 'execute failed')
  }
}

async function stopBoard(id: string) {
  try {
    await boards.stop(id)
  } catch (e: any) {
    toast.error(e?.message ?? 'stop failed')
  }
}

function boardRouteSummary(b: Board): string {
  const nodes = b.nodes ?? []
  const edges = b.edges ?? []
  if (!edges.length) return t('workspace.noEdges')
  const e = edges[0]
  const src = nodes.find((n) => n.id === e.source_id)
  const dst = nodes.find((n) => n.id === e.target_id)
  const fmt = (n?: BoardNode) => {
    if (!n) return '?'
    return n.remote_name ? `${n.remote_name}:${n.path || '/'}` : n.path || '/'
  }
  return `${fmt(src)} → ${fmt(dst)} (${e.action})`
}

// —— Flows ——
async function submitFlow() {
  if (!flowName.value.trim()) {
    toast.error(t('flows.nameRequired'))
    return
  }
  const f: Flow = {
    id: crypto.randomUUID(),
    name: flowName.value.trim(),
    schedule_cron: flowCron.value.trim() || undefined,
    enabled: flowEnabled.value,
  }
  await flows.add(f)
  showFlowForm.value = false
  flowName.value = ''
  flowCron.value = ''
  flowEnabled.value = true
  toast.success(t('workspace.flowAdded'))
}

async function deleteFlow(id: string, name: string) {
  const ok = await confirmDialog({
    title: t('flows.deleteTitle'),
    message: t('flows.deleteMessage', { name }),
    confirmText: t('common.delete'),
    confirmVariant: 'danger',
  })
  if (!ok) return
  await flows.remove(id)
}

</script>

<template>
  <div class="flex h-full flex-col" data-testid="page-workspace">
    <!-- Remotes strip (infra for all routes) — full-bleed bar, capped content -->
    <section
      class="shrink-0 border-b-2 border-border bg-bg py-3"
      data-testid="workspace-remotes"
    >
      <div class="page-content-wide">
      <div class="mb-2 flex items-center justify-between gap-2">
        <div class="flex items-center gap-2 text-xs font-bold uppercase tracking-wide text-text-muted">
          <PhCloud :size="14" weight="bold" />
          <span>{{ t('workspace.remotes') }}</span>
          <span class="badge">{{ remotes.items.length }}</span>
        </div>
        <button
          type="button"
          class="btn-secondary !px-2 !py-1 text-xs"
          data-testid="remotes-add"
          @click="showRemoteForm = !showRemoteForm"
        >
          <PhPlus :size="14" weight="bold" /> {{ t('remotes.add') }}
        </button>
      </div>

      <div v-if="showRemoteForm" class="neo-inset mb-3 p-3" data-testid="remotes-add-form">
        <form class="grid grid-cols-1 gap-2 sm:grid-cols-[1fr_1fr_auto]" @submit.prevent="submitRemote">
          <label class="field-label">
            <span>{{ t('common.name') }}</span>
            <input v-model="remoteName" required class="field-input" data-testid="remotes-name" placeholder="gdrive" />
          </label>
          <label class="field-label">
            <span>{{ t('common.type') }}</span>
            <RemoteTypeSelect v-model="remoteType" test-id="remotes-type" />
          </label>
          <div class="flex items-end">
            <button type="submit" class="btn-primary w-full sm:w-auto" data-testid="remotes-submit">
              {{ t('common.save') }}
            </button>
          </div>
        </form>
      </div>

      <div v-if="remotes.items.length" class="flex flex-wrap gap-2">
        <div
          v-for="r in remotes.items"
          :key="r.name"
          class="neo-inset flex items-center gap-2 px-2.5 py-1.5"
          :data-testid="`remote-chip-${r.name}`"
        >
          <PhCloud :size="14" weight="regular" class="text-info" />
          <span class="font-bold text-sm">{{ r.name }}</span>
          <span class="text-[11px] text-text-dim">{{ r.type }}</span>
          <button
            type="button"
            class="btn-ghost !px-1 !py-0.5 text-[11px]"
            :disabled="remoteTest[r.name]?.status === 'loading'"
            :title="remoteTestTitle(r.name)"
            :data-testid="`remotes-test-${r.name}`"
            @click="testRemote(r.name)"
          >
            <PhSpinner
              v-if="remoteTest[r.name]?.status === 'loading'"
              :size="14"
              class="animate-spin text-text-muted"
            />
            <PhCheckCircle
              v-else-if="remoteTest[r.name]?.status === 'ok'"
              :size="14"
              class="text-success"
              weight="fill"
            />
            <PhXCircle
              v-else-if="remoteTest[r.name]?.status === 'error'"
              :size="14"
              class="text-danger"
              weight="fill"
            />
            <span v-else>{{ t('common.test') }}</span>
          </button>
          <button type="button" class="btn-ghost !px-1 !py-0.5 text-danger" @click="deleteRemote(r.name)">
            <PhTrash :size="14" />
          </button>
        </div>
      </div>
      <p v-else class="text-sm text-text-muted">{{ t('workspace.noRemotes') }}</p>
      </div>
    </section>

    <!-- Scrollable workspace body -->
    <div class="min-h-0 flex-1 overflow-auto py-4 md:py-5">
      <div class="page-content-wide space-y-5">
      <AppAlert v-if="profiles.error || remotes.error || boards.error || flows.error" type="error">
        {{ profiles.error || remotes.error || boards.error || flows.error }}
      </AppAlert>

      <!-- Active tasks -->
      <section v-if="activeTasks.length" class="neo-card" data-testid="workspace-tasks">
        <div class="neo-header flex items-center gap-2">
          <PhSpinner :size="16" class="animate-spin" weight="bold" />
          <span class="font-bold">{{ t('workspace.activeTasks') }}</span>
          <span class="badge">{{ activeTasks.length }}</span>
        </div>
        <ul class="divide-y-2 divide-border">
          <li
            v-for="task in activeTasks"
            :key="task.id"
            class="flex items-center gap-2 px-3 py-2 text-sm"
          >
            <span class="font-bold">{{ task.name }}</span>
            <span class="text-text-dim">({{ task.action }})</span>
            <span class="ml-auto font-mono text-[11px] uppercase">{{ task.status }}</span>
          </li>
        </ul>
      </section>

      <!-- ========== OPERATIONS (profiles) — old "operation" units ========== -->
      <section class="neo-card" data-testid="workspace-operations">
        <div class="neo-header grid grid-cols-[minmax(0,1fr)_auto] gap-3">
          <div class="min-w-0">
            <div class="text-[10px] font-bold uppercase tracking-wide text-text/70">
              {{ t('workspace.opsLabel') }}
            </div>
            <h2 class="truncate text-lg font-bold leading-tight">{{ t('workspace.operations') }}</h2>
            <p class="mt-0.5 text-xs font-medium text-text/80">{{ t('workspace.operationsHint') }}</p>
          </div>
          <button type="button" class="btn-secondary" data-testid="profiles-add" @click="openCreateProfile">
            <PhPlus :size="14" weight="bold" /> {{ t('workspace.addOperation') }}
          </button>
        </div>

        <div class="space-y-2 bg-bg p-3">
          <div v-if="showProfileForm" class="neo-inset p-3" data-testid="profiles-add-form">
            <h3 class="section-label">
              {{ profileMode === 'create' ? t('profiles.new') : t('profiles.edit') }}
            </h3>
            <form class="grid grid-cols-1 gap-3 md:grid-cols-2" @submit.prevent="submitProfile">
              <label class="field-label">
                <span>{{ t('common.name') }}</span>
                <input
                  v-model="profileDraft.name"
                  class="field-input"
                  :class="fieldError('name') && 'border-danger'"
                  :disabled="profileMode === 'edit'"
                  data-testid="profiles-name"
                  @focus="touchProfileField('name')"
                />
                <p v-if="fieldError('name')" class="field-error" data-testid="profiles-error-name">{{ fieldError('name') }}</p>
              </label>
              <label class="field-label">
                <span>{{ t('profiles.direction') }}</span>
                <DirectionField
                  v-model="profileDraft.direction"
                  :invalid="!!fieldError('direction')"
                  test-id="profiles-direction"
                  @focus="touchProfileField('direction')"
                />
                <p v-if="fieldError('direction')" class="field-error">{{ fieldError('direction') }}</p>
              </label>
              <div class="field-label md:col-span-2" @focusin="touchProfileField('from')">
                <span>{{ t('profiles.fromLabel') }}</span>
                <RemotePathField v-model="profileDraft.from" :remotes="remotes.items" test-id="profiles-from" />
                <p v-if="fieldError('from')" class="field-error" data-testid="profiles-error-from">{{ fieldError('from') }}</p>
              </div>
              <div class="field-label md:col-span-2" @focusin="touchProfileField('to')">
                <span>{{ t('profiles.toLabel') }}</span>
                <RemotePathField v-model="profileDraft.to" :remotes="remotes.items" test-id="profiles-to" />
                <p v-if="fieldError('to')" class="field-error" data-testid="profiles-error-to">{{ fieldError('to') }}</p>
              </div>
              <label class="field-label">
                <span>{{ t('profiles.parallel') }}</span>
                <input
                  v-model.number="profileDraft.parallel"
                  type="number"
                  min="0"
                  class="field-input"
                  :class="fieldError('parallel') && 'border-danger'"
                  @focus="touchProfileField('parallel')"
                />
                <p v-if="fieldError('parallel')" class="field-error">{{ fieldError('parallel') }}</p>
              </label>
              <div class="flex items-end">
                <AppCheckbox v-model="profileDraft.dry_run!" :label="t('profiles.dryRun')" />
              </div>

              <AppAlert v-if="profiles.error" type="error" class="md:col-span-2">{{ profiles.error }}</AppAlert>

              <div class="flex gap-2 md:col-span-2">
                <button
                  type="submit"
                  class="btn-primary"
                  data-testid="profiles-submit"
                  :disabled="!profileFormValid || profiles.loading"
                >
                  {{ t('common.save') }}
                </button>
                <button type="button" class="btn-secondary" @click="showProfileForm = false">{{ t('common.cancel') }}</button>
              </div>
            </form>
          </div>

          <template v-if="profiles.items.length">
            <article
              v-for="(p, i) in profiles.items"
              :key="p.name"
              class="neo-inset"
              :data-testid="`profile-row-${p.name}`"
            >
              <div
                class="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 border-b-2 border-border bg-bg-secondary px-3 py-2"
              >
                <span class="font-mono text-xs font-bold text-text-dim">#{{ i + 1 }}</span>
                <div class="min-w-0">
                  <div class="truncate font-bold">{{ p.name }}</div>
                  <div class="mt-0.5 flex flex-wrap items-center gap-1 text-xs text-text-muted">
                    <span class="font-mono truncate">{{ p.from }}</span>
                    <PhArrowRight :size="12" weight="bold" class="shrink-0 text-text-dim" />
                    <span class="font-mono truncate">{{ p.to }}</span>
                    <span class="badge ml-1">{{ p.direction || 'push' }}</span>
                  </div>
                </div>
                <div class="flex shrink-0 items-center gap-1.5">
                  <button type="button" class="btn-primary !px-2 !py-1" data-testid="ops-run" @click="runProfile(p)">
                    <PhPlay :size="14" weight="bold" /> {{ t('workspace.run') }}
                  </button>
                  <button type="button" class="btn-secondary !px-2 !py-1" @click="openEditProfile(p)">
                    <PhPencilSimple :size="14" weight="bold" />
                  </button>
                  <button type="button" class="btn-danger !px-2 !py-1" @click="deleteProfile(p.name)">
                    <PhTrash :size="14" />
                  </button>
                </div>
              </div>
              <button
                type="button"
                class="flex w-full items-center justify-center gap-1 py-1 text-[11px] font-bold uppercase text-text-dim hover:bg-surface-hover"
                @click="expandedProfile = expandedProfile === p.name ? null : p.name"
              >
                {{ expandedProfile === p.name ? t('workspace.collapse') : t('workspace.details') }}
              </button>
              <div v-if="expandedProfile === p.name" class="border-t-2 border-border px-3 py-2 text-xs text-text-muted">
                <div>{{ t('profiles.parallel') }}: {{ p.parallel ?? 4 }} · {{ t('profiles.bandwidth') }}: {{ p.bandwidth ?? 0 }}</div>
                <div v-if="p.dry_run" class="text-warning">{{ t('profiles.dryRun') }}</div>
              </div>
            </article>
            <div v-if="profiles.items.length > 1" class="flex justify-center py-1 text-text-dim">
              <PhArrowDown :size="16" />
            </div>
          </template>
          <div
            v-else
            class="neo-dashed flex cursor-pointer items-center justify-center gap-2 p-4"
            role="button"
            tabindex="0"
            @click="openCreateProfile"
            @keydown.enter="openCreateProfile"
          >
            <PhPlus :size="16" class="text-text-dim" />
            <span class="text-sm font-medium text-text-muted">{{ t('workspace.addOperation') }}</span>
          </div>
        </div>
      </section>

      <!-- ========== BOARDS ========== -->
      <section class="neo-card" data-testid="workspace-boards">
        <div class="neo-header grid grid-cols-[minmax(0,1fr)_auto] gap-3">
          <div class="min-w-0">
            <div class="text-[10px] font-bold uppercase tracking-wide text-text/70">
              {{ t('workspace.boardsLabel') }}
            </div>
            <h2 class="flex items-center gap-2 text-lg font-bold leading-tight">
              <PhSquaresFour :size="20" weight="bold" />
              {{ t('workspace.boards') }}
            </h2>
            <p class="mt-0.5 text-xs font-medium text-text/80">{{ t('workspace.boardsHint') }}</p>
          </div>
          <button type="button" class="btn-secondary" data-testid="boards-add" @click="showBoardForm = !showBoardForm">
            <PhPlus :size="14" weight="bold" /> {{ t('boards.add') }}
          </button>
        </div>

        <div class="space-y-2 bg-bg p-3">
          <div v-if="showBoardForm" class="neo-inset p-3" data-testid="boards-add-form">
            <form class="grid grid-cols-1 gap-3" @submit.prevent="submitBoard">
              <label class="field-label">
                <span>{{ t('common.name') }}</span>
                <input v-model="boardName" required class="field-input" data-testid="boards-name" />
              </label>
              <label class="field-label">
                <span>{{ t('boards.source') }}</span>
                <RemotePathField v-model="boardSource" :remotes="remotes.items" test-id="boards-source" />
              </label>
              <label class="field-label">
                <span>{{ t('boards.target') }}</span>
                <RemotePathField v-model="boardTarget" :remotes="remotes.items" test-id="boards-target" />
              </label>
              <label class="field-label">
                <span>{{ t('common.action') }}</span>
                <select v-model="boardAction" class="field-input" data-testid="boards-action">
                  <option v-for="a in BOARD_EDGE_ACTIONS" :key="a" :value="a">{{ a }}</option>
                </select>
              </label>
              <div class="flex gap-2">
                <button type="submit" class="btn-primary" data-testid="boards-submit">{{ t('common.save') }}</button>
                <button type="button" class="btn-secondary" @click="showBoardForm = false">{{ t('common.cancel') }}</button>
              </div>
            </form>
          </div>

          <article
            v-for="b in boards.items"
            :key="b.id"
            class="neo-inset"
            :data-testid="`board-card-${b.id}`"
          >
            <div class="flex flex-wrap items-center gap-3 px-3 py-3">
              <div class="min-w-0 flex-1">
                <div class="font-bold">{{ b.name }}</div>
                <div class="mt-1 font-mono text-xs text-text-muted">{{ boardRouteSummary(b) }}</div>
                <div v-if="boards.lastRun[b.id]" class="mt-1 text-[11px] font-bold uppercase text-text-dim">
                  {{ boards.lastRun[b.id].status }}
                </div>
              </div>
              <div class="flex gap-1.5">
                <button
                  v-if="boards.lastRun[b.id]?.status === 'running'"
                  type="button"
                  class="btn-danger !px-2 !py-1"
                  @click="stopBoard(b.id)"
                >
                  <PhStop :size="14" weight="bold" /> {{ t('workspace.stop') }}
                </button>
                <button v-else type="button" class="btn-primary !px-2 !py-1" @click="runBoard(b.id)">
                  <PhPlay :size="14" weight="bold" /> {{ t('workspace.run') }}
                </button>
                <button type="button" class="btn-danger !px-2 !py-1" @click="deleteBoard(b.id, b.name)">
                  <PhTrash :size="14" />
                </button>
              </div>
            </div>
          </article>

          <div
            v-if="!boards.items.length && !showBoardForm"
            class="neo-dashed flex cursor-pointer items-center justify-center gap-2 p-4"
            role="button"
            tabindex="0"
            @click="showBoardForm = true"
            @keydown.enter="showBoardForm = true"
          >
            <PhPlus :size="16" class="text-text-dim" />
            <span class="text-sm font-medium text-text-muted">{{ t('boards.add') }}</span>
          </div>
        </div>
      </section>

      <!-- ========== FLOWS ========== -->
      <section
        v-for="(f, fi) in flows.items"
        :key="f.id"
        class="neo-card"
        :data-testid="`flow-card-${f.id}`"
      >
        <div class="neo-header grid grid-cols-[minmax(0,1fr)_auto] gap-3">
          <div class="min-w-0">
            <div class="text-[10px] font-bold uppercase tracking-wide text-text/70">
              {{ t('workspace.flowLabel', { n: fi + 1 }) }}
            </div>
            <h2 class="flex items-center gap-2 truncate text-lg font-bold leading-tight">
              <PhStack :size="20" weight="bold" />
              {{ f.name || t('workspace.untitledFlow') }}
            </h2>
            <div class="mt-1 flex flex-wrap gap-2">
              <span v-if="f.schedule_cron" class="badge">
                <PhClock :size="12" class="mr-0.5 inline" /> {{ f.schedule_cron }}
              </span>
              <span v-else class="badge">{{ t('flows.noSchedule') }}</span>
              <span class="badge" :class="f.enabled ? 'text-success' : ''">
                {{ f.enabled ? t('common.enabled') : t('common.disabled') }}
              </span>
            </div>
          </div>
          <button type="button" class="btn-danger !px-2 !py-1" :data-testid="`flows-delete-${f.id}`" @click="deleteFlow(f.id, f.name)">
            <PhTrash :size="14" /> {{ t('workspace.remove') }}
          </button>
        </div>
        <div class="bg-bg p-3 text-sm text-text-muted">
          {{ t('workspace.flowBody') }}
        </div>
      </section>

      <!-- Add flow -->
      <div
        class="neo-dashed flex cursor-pointer items-center justify-center gap-2 p-3 transition-colors hover:border-accent-strong hover:bg-accent/10"
        role="button"
        tabindex="0"
        data-testid="flows-add"
        @click="showFlowForm = !showFlowForm"
        @keydown.enter="showFlowForm = !showFlowForm"
      >
        <PhPlus :size="16" class="text-text-dim" />
        <span class="text-sm font-medium text-text-muted">{{ t('flows.add') }}</span>
      </div>

      <div v-if="showFlowForm" class="neo-card p-4" data-testid="flows-add-form">
        <form class="grid grid-cols-1 gap-3 md:grid-cols-2" @submit.prevent="submitFlow">
          <label class="field-label">
            <span>{{ t('common.name') }}</span>
            <input v-model="flowName" required class="field-input" data-testid="flows-name" />
          </label>
          <label class="field-label">
            <span>{{ t('flows.schedule') }}</span>
            <CronField v-model="flowCron" test-id="flows-cron" allow-none />
          </label>
          <div class="flex items-end">
            <AppCheckbox v-model="flowEnabled" :label="t('common.enabled')" />
          </div>
          <div class="flex gap-2 md:col-span-2">
            <button type="submit" class="btn-primary" data-testid="flows-submit">{{ t('common.save') }}</button>
            <button type="button" class="btn-secondary" @click="showFlowForm = false">{{ t('common.cancel') }}</button>
          </div>
        </form>
      </div>

      <p v-if="loading" class="text-center text-sm text-text-muted">{{ t('common.loading') }}…</p>
      </div>
    </div>
  </div>
</template>
