import { computed, ref, watch } from 'vue'
import { defineStore } from 'pinia'

export type ThemePreference = 'light' | 'dark' | 'system'

const STORAGE_KEY = 'gn-drive:theme'
const DEFAULT_THEME: ThemePreference = 'dark'

function readStored(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY)
    if (v === 'light' || v === 'dark' || v === 'system') return v
  } catch {
    // ignore
  }
  return DEFAULT_THEME
}

function resolveIsDark(preference: ThemePreference): boolean {
  if (preference === 'dark') return true
  if (preference === 'light') return false
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

function applyDom(isDark: boolean) {
  const root = document.documentElement
  if (isDark) root.classList.add('dark')
  else root.classList.remove('dark')
}

export const useThemeStore = defineStore('theme', () => {
  const preference = ref<ThemePreference>(readStored())
  const isDark = computed(() => resolveIsDark(preference.value))

  function setTheme(t: ThemePreference | 'light' | 'dark') {
    preference.value = t
  }

  function toggle() {
    setTheme(isDark.value ? 'light' : 'dark')
  }

  watch(
    [preference, isDark],
    () => {
      try {
        localStorage.setItem(STORAGE_KEY, preference.value)
      } catch {
        // ignore
      }
      applyDom(isDark.value)
    },
    { immediate: true },
  )

  return { preference, isDark, setTheme, toggle }
})
