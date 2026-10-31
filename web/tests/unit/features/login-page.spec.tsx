import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { QueryClientProvider } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'
import * as TooltipPrimitive from '@radix-ui/react-tooltip'
import { LoginPage } from '@/features/auth/login-page'
import { ThemeProvider } from '@/components/theme/theme'
import { resetSessionStore } from '@/api/session-store'
import { resetAuthenticatedHistory } from '@/api/session-history'
import type { WebSession } from '@/api/session'
import { jsonResponse, testQueryClient } from '../api/test-utils'

const localTrusted: WebSession = {
  mode: 'local-trusted',
  device_id: 'local-trusted',
  display_name: 'Local browser',
  expires_at_ms: null,
}

const authenticated: WebSession = {
  mode: 'authenticated',
  device_id: 'dev-1',
  display_name: null,
  expires_at_ms: 1_700_000_000_000,
}

function renderLoginPage(
  client: QueryClient,
  initialEntries: Array<string | { pathname: string; state?: unknown }>,
) {
  return render(
    <ThemeProvider>
      <QueryClientProvider client={client}>
        <TooltipPrimitive.Provider>
          <MemoryRouter initialEntries={initialEntries}>
            <Routes>
              <Route path="/login" element={<LoginPage />} />
              <Route path="/projects/p-1" element={<p>Project workspace</p>} />
            </Routes>
          </MemoryRouter>
        </TooltipPrimitive.Provider>
      </QueryClientProvider>
    </ThemeProvider>,
  )
}

describe('LoginPage in each auth mode', () => {
  beforeEach(() => {
    resetSessionStore()
    resetAuthenticatedHistory()
  })

  afterEach(() => {
    resetSessionStore()
    resetAuthenticatedHistory()
    vi.unstubAllGlobals()
  })

  it('shows the device form in authenticated mode', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(authenticated)))
    const client = testQueryClient()

    renderLoginPage(client, ['/login'])

    expect(await screen.findByLabelText('Device ID')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Sign in to Orbit' })).toBeInTheDocument()
    expect(screen.getByLabelText('Device token')).toBeInTheDocument()
  })

  it('redirects to the intended destination without rendering the form when local-trusted', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(localTrusted)))
    const client = testQueryClient()

    renderLoginPage(client, [{ pathname: '/login', state: { from: '/projects/p-1' } }])

    expect(await screen.findByText('Project workspace')).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByLabelText('Device ID')).not.toBeInTheDocument())
    expect(screen.queryByRole('heading', { name: 'Sign in to Orbit' })).not.toBeInTheDocument()
  })
})
