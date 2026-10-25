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
    setupFiles: ['./src/app/test-setup.ts'],
    // Vitest owns the unit lane only. The Playwright specs in `tests/e2e` run
    // from the project-root `playwright.config.ts` and would fail here at
    // import time (`fileURLToPath(new URL(...))` under jsdom).
    include: ['src/**/*.test.{ts,tsx}', 'tests/unit/**/*.spec.{ts,tsx}'],
    exclude: ['tests/e2e/**', 'node_modules/**', 'dist/**'],
    css: false,
    restoreMocks: true,
  },
})
