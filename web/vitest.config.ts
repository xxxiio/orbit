import { fileURLToPath, URL } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./tests/setup.ts'],
    // Vitest owns the unit lane only, under one root. The Playwright specs in
    // `tests/e2e` run from the project-root `playwright.config.ts` and would
    // fail here at import time under jsdom.
    include: ['tests/unit/**/*.{test,spec}.{ts,tsx}'],
    exclude: ['tests/e2e/**', 'node_modules/**', 'dist/**'],
    css: false,
    restoreMocks: true,
  },
})
