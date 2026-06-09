import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
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
