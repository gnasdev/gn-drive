import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'
import IstanbulPlugin from 'vite-plugin-istanbul'
import { VitePWA } from 'vite-plugin-pwa'

const coverage = process.env.E2E_COVERAGE === '1'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    tailwindcss(),
    VitePWA({
      registerType: 'autoUpdate',
      manifest: false,
      // Same posture as the other GNAS frontends: emit a self-destroying
      // worker instead of an active Workbox cache. gn-drive's data (rclone
      // operations, transfer status) changes continuously, so caching it
      // would show stale state; this only guards against any previously
      // installed worker outliving a new deploy.
      selfDestroying: true,
    }),
    ...(coverage
      ? [
          IstanbulPlugin({
            include: 'src/*',
            exclude: ['node_modules', 'e2e', 'src/api/types.ts'],
            extension: ['.js', '.ts', '.vue'],
            requireEnv: false,
            cypress: false,
            checkProd: false,
            forceBuildInstrument: true,
          }),
        ]
      : []),
  ],
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
