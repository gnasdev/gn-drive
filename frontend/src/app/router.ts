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
    name: 'workspace',
    component: () => import('@/pages/WorkspacePage.vue'),
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/pages/SettingsPage.vue'),
  },
  // Legacy multi-page routes → single workspace (pre-Vue desktop model)
  { path: '/dashboard', redirect: '/' },
  { path: '/profiles', redirect: '/' },
  { path: '/remotes', redirect: '/' },
  { path: '/operations', redirect: '/' },
  { path: '/boards', redirect: '/' },
  { path: '/flows', redirect: '/' },
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
  if (!to.meta.public && !auth.unlocked) {
    return { name: 'unlock' }
  }
  if (to.name === 'unlock' && auth.unlocked && auth.setup) {
    return { name: 'workspace' }
  }
  return true
})
