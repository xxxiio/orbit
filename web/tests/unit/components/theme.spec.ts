import { afterEach, describe, expect, it } from 'vitest'
import {
  applyThemeToDocument,
  readThemePreference,
  resolveTheme,
  THEME_STORAGE_KEY,
} from '@/components/theme/theme'

afterEach(() => {
  document.documentElement.removeAttribute('data-theme')
  document.documentElement.classList.remove('dark')
})

describe('theme resolution', () => {
  it('honours a stored preference and falls back to the system', () => {
    expect(resolveTheme('light', true)).toBe('light')
    expect(resolveTheme('dark', false)).toBe('dark')
    expect(resolveTheme('system', true)).toBe('dark')
    expect(resolveTheme('system', false)).toBe('light')

    expect(readThemePreference({ getItem: (key) => (key === THEME_STORAGE_KEY ? 'dark' : null) })).toBe(
      'dark',
    )
    expect(readThemePreference({ getItem: () => 'sepia' })).toBe('system')
    expect(readThemePreference({ getItem: () => null })).toBe('system')

    applyThemeToDocument('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(document.documentElement.classList.contains('dark')).toBe(true)

    applyThemeToDocument('light')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })
})
