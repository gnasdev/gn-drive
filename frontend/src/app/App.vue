<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { useAuthStore } from '@/stores/auth'
import AppSidebar from '@/components/layout/Sidebar.vue'
import AppTopbar from '@/components/layout/Topbar.vue'
import DialogHost from '@/components/ui/DialogHost.vue'
import ToastHost from '@/components/ui/ToastHost.vue'

const auth = useAuthStore()
const route = useRoute()

const showLayout = computed(() => auth.unlocked && route.name !== 'unlock')
</script>

<template>
  <div class="flex h-dvh w-full bg-bg">
    <template v-if="showLayout">
      <AppSidebar />
      <div class="flex min-w-0 flex-1 flex-col">
        <AppTopbar />
        <main class="flex-1 overflow-auto px-8 py-6">
          <RouterView />
        </main>
      </div>
    </template>
    <template v-else>
      <RouterView />
    </template>
  </div>
  <DialogHost />
  <ToastHost />
</template>
