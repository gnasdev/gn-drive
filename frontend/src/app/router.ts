import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import { useAuthStore } from '@/stores/auth'

const routes: RouteRecordRaw[] = [
  {
    path: '/unlock',
    name: 'unlock',
    component: () => import('@/pages/UnlockPage.vue'),
    meta: { public: true },
  },
  {
    path: '/',
    name: 'dashboard',
    component: () => import('@/pages/DashboardPage.vue'),
  },
  {
    path: '/profiles',
    name: 'profiles',
    component: () => import('@/pages/ProfilesPage.vue'),
  },
  {
    path: '/remotes',
    name: 'remotes',
    component: () => import('@/pages/RemotesPage.vue'),
  },
  {
    path: '/operations',
    name: 'operations',
    component: () => import('@/pages/OperationsPage.vue'),
  },
  {
    path: '/boards',
    name: 'boards',
    component: () => import('@/pages/BoardsPage.vue'),
  },
  {
    path: '/flows',
    name: 'flows',
    component: () => import('@/pages/FlowsPage.vue'),
  },
  {
    path: '/schedules',
    name: 'schedules',
    component: () => import('@/pages/SchedulesPage.vue'),
  },
  {
    path: '/history',
    name: 'history',
    component: () => import('@/pages/HistoryPage.vue'),
  },
  {
    path: '/service',
    name: 'service',
    component: () => import('@/pages/ServicePage.vue'),
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/pages/SettingsPage.vue'),
  },
  // SPA fallback
  { path: '/:pathMatch(.*)*', redirect: '/' },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
})

router.beforeEach(async (to) => {
  const auth = useAuthStore()
  if (!auth.initialized) {
    await auth.fetchStatus()
  }
  // When master password is not configured yet, keep /unlock reachable so
  // first-run setup works even though the app is open (unlocked=true).
  if (!to.meta.public && !auth.unlocked) {
    return { name: 'unlock' }
  }
  if (to.name === 'unlock' && auth.unlocked && auth.setup) {
    return { name: 'dashboard' }
  }
  return true
})
