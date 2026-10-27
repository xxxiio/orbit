import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { NotesSection } from '@/features/tasks/task-drawer/notes-section'
import { jsonResponse, testQueryClient } from '../api/test-utils'

function renderNotes(projectId?: string) {
  return render(
    <MemoryRouter>
      <QueryClientProvider client={testQueryClient()}>
        <NotesSection taskId="t-1" projectId={projectId} />
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

function stubKnowledge(
  references: (input: string) => Response,
  inventory: unknown[] = [],
) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/authoritative/markdown-references')) return references(url)
      if (url.includes('/authoritative/markdown-inventory')) return jsonResponse(inventory)
      return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('task drawer notes section', () => {
  it('lists the notes that reference the task and links them with their context', async () => {
    stubKnowledge(() =>
      jsonResponse({
        target: 'task:t-1',
        notes: [{ id: 'n-1', path: 'Work/Release.md', title: 'Release notes' }],
      }),
    )
    renderNotes('p-1')

    const link = await screen.findByRole('link', { name: 'Release notes' })
    expect(link).toHaveAttribute('href', '/knowledge/Work/Release.md?project=p-1&task=t-1')
    expect(screen.getByRole('button', { name: 'Unlink Release notes from this task' })).toBeInTheDocument()
    expect(screen.getByText('Work/Release.md')).toBeInTheDocument()
  })

  it('says no note references the task when the server returns none', async () => {
    stubKnowledge(() => jsonResponse({ target: 'task:t-1', notes: [] }))
    renderNotes()

    expect(await screen.findByText(/No note references this task yet/)).toBeInTheDocument()
  })

  it('reports an older server as unavailable instead of empty', async () => {
    stubKnowledge(() => jsonResponse({ error: 'not found' }, 404))
    renderNotes()

    expect(await screen.findByText(/does not provide note references yet/)).toBeInTheDocument()
    expect(screen.queryByText(/No note references this task yet/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Link a note/ })).toBeDisabled()
  })

  it('shows the server refusal when the reference read is refused', async () => {
    stubKnowledge(() => jsonResponse({ error: 'invalid reference target' }, 400))
    renderNotes()

    expect(await screen.findByText(/could not be read/)).toBeInTheDocument()
    expect(screen.getByText(/invalid reference target/)).toBeInTheDocument()
  })
})
