<script setup lang="ts">
import { computed, provide } from 'vue'
import { useRoute } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import { useEventStream } from '@/composables/useEventStream'
import AppTopbar from '@/components/layout/Topbar.vue'
import DialogHost from '@/components/ui/DialogHost.vue'
import ToastHost from '@/components/ui/ToastHost.vue'

const auth = useAuthStore()
const route = useRoute()

/** Unlocked app uses single-page shell (topbar + main), like desktop v0.4. */
const showLayout = computed(() => auth.unlocked && route.name !== 'unlock')

/** SSE at app shell so flow/run events keep flowing even when Workspace is deactivated. */
const sseEnabled = computed(() => auth.unlocked)
const { connected: eventsConnected } = useEventStream({ enabled: sseEnabled })
provide('eventsConnected', eventsConnected)
</script>

<template>
  <div class="flex h-full max-h-full w-full flex-col overflow-hidden bg-bg-secondary">
    <template v-if="showLayout">
      <AppTopbar />
      <main class="min-h-0 flex-1 overflow-hidden">
        <!-- KeepAlive preserves Workspace local state (open forms, drafts) when
             navigating to Settings and back. Page roots scroll inside main. -->
        <RouterView v-slot="{ Component }">
          <KeepAlive :include="['WorkspacePage']">
            <component :is="Component" />
          </KeepAlive>
        </RouterView>
      </main>
    </template>
    <template v-else>
      <div class="min-h-0 flex-1 overflow-auto">
        <RouterView />
      </div>
    </template>
  </div>
  <DialogHost />
  <ToastHost />
</template>
