import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

type Theme = 'dark' | 'light'

const STORAGE_KEY = 'gn-drive:theme'

function initial(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY) as Theme | null
  if (stored === 'dark' || stored === 'light') return stored
  return 'dark' // dark by default per plan section 6.4
}

export const useThemeStore = defineStore('theme', () => {
  const mode = ref<Theme>(initial())

  function apply(t: Theme) {
    if (typeof document === 'undefined') return
    const root = document.documentElement
    if (t === 'light') {
      root.classList.add('light')
    } else {
      root.classList.remove('light')
    }
  }

  function toggle() {
    mode.value = mode.value === 'dark' ? 'light' : 'dark'
  }

  function set(t: Theme) {
    mode.value = t
  }

  // Apply on init
  apply(mode.value)
  watch(mode, (t) => {
    localStorage.setItem(STORAGE_KEY, t)
    apply(t)
  })

  return { mode, toggle, set }
})
