import { createI18n } from 'vue-i18n'
import en from './locales/en'
import vi from './locales/vi'

export type AppLocale = 'en' | 'vi'

export const LOCALE_STORAGE_KEY = 'gn-drive:locale'

export const SUPPORTED_LOCALES: AppLocale[] = ['en', 'vi']

export function readStoredLocale(): AppLocale {
  try {
    const v = localStorage.getItem(LOCALE_STORAGE_KEY)
    if (v === 'en' || v === 'vi') return v
  } catch {
    // ignore
  }
  // Prefer browser language when available.
  try {
    const nav = navigator.language?.toLowerCase() ?? ''
    if (nav.startsWith('vi')) return 'vi'
  } catch {
    // ignore
  }
  return 'en'
}

export const i18n = createI18n({
  legacy: false,
  locale: readStoredLocale(),
  fallbackLocale: 'en',
  messages: {
    en,
    vi,
  },
})

export function setAppLocale(locale: AppLocale) {
  i18n.global.locale.value = locale
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale)
  } catch {
    // ignore
  }
  if (typeof document !== 'undefined') {
    document.documentElement.lang = locale === 'vi' ? 'vi' : 'en'
  }
}

// Apply html lang on boot.
setAppLocale(readStoredLocale())
