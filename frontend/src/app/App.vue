<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import AppTopbar from '@/components/layout/Topbar.vue'
import DialogHost from '@/components/ui/DialogHost.vue'
import ToastHost from '@/components/ui/ToastHost.vue'

const auth = useAuthStore()
const route = useRoute()

/** Unlocked app uses single-page shell (topbar + main), like desktop v0.4. */
const showLayout = computed(() => auth.unlocked && route.name !== 'unlock')
</script>

<template>
  <div class="flex h-dvh w-full flex-col bg-bg-secondary">
    <template v-if="showLayout">
      <AppTopbar />
      <main class="min-h-0 flex-1 overflow-hidden">
        <!-- KeepAlive preserves Workspace local state (open forms, drafts) when
             navigating to Settings and back. -->
        <RouterView v-slot="{ Component }">
          <KeepAlive :include="['WorkspacePage']">
            <component :is="Component" />
          </KeepAlive>
        </RouterView>
      </main>
    </template>
    <template v-else>
      <RouterView />
    </template>
  </div>
  <DialogHost />
  <ToastHost />
</template>
