// GN Drive theme store, built on the shared theme factory so dark mode is
// driven by the `dark` class on <html> (same convention as @gnas/ui-shared
// components). Exposes { isDark, preference, toggle, setTheme }.
import { createThemeStore } from '@gnas/ui-shared'

export const useThemeStore = createThemeStore({
  storageKey: 'gn-drive:theme',
  defaultTheme: 'dark',
})
