import { computed } from 'vue'
import { defineStore } from 'pinia'
import {
  type AppLocale,
  i18n,
  readStoredLocale,
  setAppLocale,
  SUPPORTED_LOCALES,
} from '@/i18n'

export const useLocaleStore = defineStore('locale', () => {
  const locale = computed<AppLocale>(() => i18n.global.locale.value as AppLocale)
  const locales = SUPPORTED_LOCALES

  function setLocale(next: AppLocale) {
    setAppLocale(next)
  }

  // Ensure store and i18n stay aligned with storage on first use.
  if (locale.value !== readStoredLocale()) {
    setAppLocale(readStoredLocale())
  }

  return { locale, locales, setLocale }
})
