<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import AppSidebar from '@/components/layout/Sidebar.vue'
import AppTopbar from '@/components/layout/Topbar.vue'
import { AppDialogProvider } from '@gnas/ui-shared'
import ToastContainer from '@/components/ui/ToastContainer.vue'

const auth = useAuthStore()
const route = useRoute()

const showLayout = computed(() => auth.unlocked && route.name !== 'unlock')
</script>

<template>
  <div class="app-shell">
    <template v-if="showLayout">
      <AppSidebar />
      <div class="app-main">
        <AppTopbar />
        <main class="app-content">
          <RouterView />
        </main>
      </div>
    </template>
    <template v-else>
      <RouterView />
    </template>
  </div>
  <!-- Global overlays from @gnas/ui-shared (confirm/alert dialogs + toasts) -->
  <AppDialogProvider />
  <ToastContainer />
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100vh;
  width: 100%;
  background: var(--color-bg);
}
.app-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.app-content {
  flex: 1;
  overflow: auto;
  padding: 24px 32px;
}
</style>
