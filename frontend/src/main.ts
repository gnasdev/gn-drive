import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './app/App.vue'
import { router } from './app/router'
import { i18n } from './i18n'
import { useThemeStore } from './stores/theme'
import { useLocaleStore } from './stores/locale'
import '@fontsource-variable/geist/index.css'
import './styles/main.css'

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)
app.use(i18n)
// Apply persisted theme class on <html> before first paint of layout chrome.
useThemeStore(pinia)
useLocaleStore(pinia)
app.use(router)
app.mount('#app')
