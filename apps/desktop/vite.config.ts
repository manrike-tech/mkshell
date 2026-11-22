import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Configuración recomendada por Tauri: puerto fijo y sin limpiar la consola.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  build: {
    target: ['es2022', 'chrome110', 'safari16'],
    sourcemap: false,
    chunkSizeWarningLimit: 1500,
  },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
