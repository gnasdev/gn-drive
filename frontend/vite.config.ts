import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    // dedupe is critical: @gnas/ui-shared is imported from source, so without
    // deduping these singletons Vite could bundle two copies of Vue/Pinia and
    // break provide/inject + the active Pinia instance.
    dedupe: ['vue', 'vue-router', 'pinia'],
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
      // Resolve the shared UI library to its source (same pattern as the other
      // GNAS frontends) so Vite/Tailwind compile its .vue/.ts directly.
      '@gnas/ui-shared': fileURLToPath(new URL('../../gn-ui-shared/src', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    strictPort: false,
    proxy: {
      // Proxy /api/* to the Go backend during dev.
      // The Go server picks a random port (127.0.0.1:0); we discover it
      // via the /api/v1/status response or rely on the user setting
      // GN_DRIVE_DEV_PORT. For now, the proxy is wired but optional.
      '/api': {
        target: process.env.GN_DRIVE_DEV_PORT
          ? `http://127.0.0.1:${process.env.GN_DRIVE_DEV_PORT}`
          : 'http://127.0.0.1:53241',
        changeOrigin: false,
        ws: false,
      },
      '/events': {
        target: process.env.GN_DRIVE_DEV_PORT
          ? `http://127.0.0.1:${process.env.GN_DRIVE_DEV_PORT}`
          : 'http://127.0.0.1:53241',
        changeOrigin: false,
        ws: false,
      },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: true,
  },
})
