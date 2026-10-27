import { afterEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { NewFolderDialog } from '@/features/knowledge/folder/folder-create-dialog'
import { FolderMoveDialog } from '@/features/knowledge/folder/folder-move-dialog'
import { folderOptions } from '@/features/knowledge/folder-model'
import type { KnowledgeNode } from '@/api/contracts'
import { jsonResponse, testQueryClient } from '../api/test-utils'

const tree: KnowledgeNode = {
  kind: 'folder',
  name: 'vault',
  path: '',
  children: [{ kind: 'folder', name: 'Work', path: 'Work', children: [] }],
}

interface SentBody {
  command_id: string
  command: { path: string }
}

/** A fetch stub that records every folder-create request body. */
function stubCreate(response: () => Response, sent: SentBody[]) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.includes('/authoritative/folder-creates')) {
        sent.push(JSON.parse(String(init?.body)) as SentBody)
        return response()
      }
      return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )
}

function renderDialog(initialParent = 'Work', onCreated = vi.fn()) {
  render(
    <MemoryRouter>
      <QueryClientProvider client={testQueryClient()}>
        <NewFolderDialog
          options={folderOptions(tree)}
          initialParent={initialParent}
          onClose={() => undefined}
          onCreated={onCreated}
          onRefreshTree={() => undefined}
        />
      </QueryClientProvider>
    </MemoryRouter>,
  )
  return onCreated
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('new folder dialog', () => {
  it('composes the path from the picker and one name', async () => {
    const sent: SentBody[] = []
    stubCreate(() => jsonResponse({ command_id: 'c-1', path: 'Work/Research' }), sent)
    const onCreated = renderDialog()

    await userEvent.type(screen.getByLabelText(/Folder name/), 'Research')
    expect(screen.getByText('Work/Research')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Create folder' }))

    await waitFor(() => expect(sent).toHaveLength(1))
    expect(sent[0].command).toEqual({ path: 'Work/Research' })
    expect(sent[0].command_id).toMatch(/^[A-Za-z0-9_-]{1,128}$/)
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith('Work/Research'))
  })

  it('quotes the server refusal and replays the same command id on retry', async () => {
    const sent: SentBody[] = []
    stubCreate(() => jsonResponse({ error: 'invalid folder creation' }, 400), sent)
    renderDialog()

    await userEvent.type(screen.getByLabelText(/Folder name/), 'Research')
    await userEvent.click(screen.getByRole('button', { name: 'Create folder' }))
    expect(await screen.findByText(/invalid folder creation/)).toBeInTheDocument()
    expect(screen.getByText(/re-read rather than guessed/)).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: 'Create folder' }))
    await waitFor(() => expect(sent).toHaveLength(2))
    expect(sent[1].command_id).toBe(sent[0].command_id)
  })

  it('marks an unanswered create as uncertain and retries it unchanged', async () => {
    const sent: SentBody[] = []
    stubCreate(() => jsonResponse({ error: 'storage error' }, 500), sent)
    renderDialog()

    await userEvent.type(screen.getByLabelText(/Folder name/), 'Research')
    await userEvent.click(screen.getByRole('button', { name: 'Create folder' }))

    expect(await screen.findByText(/may or may not have been created/)).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Retry the same folder' }))

    await waitFor(() => expect(sent).toHaveLength(2))
    expect(sent[1].command_id).toBe(sent[0].command_id)
    expect(sent[1].command.path).toBe(sent[0].command.path)
  })

  it('refuses a name with a slash before sending anything', async () => {
    const sent: SentBody[] = []
    stubCreate(() => jsonResponse({ command_id: 'c-1', path: 'Work/a/b' }), sent)
    renderDialog()

    await userEvent.type(screen.getByLabelText(/Folder name/), 'a/b')
    await userEvent.click(screen.getByRole('button', { name: 'Create folder' }))

    expect(screen.getByText(/without slashes/)).toBeInTheDocument()
    expect(sent).toHaveLength(0)
  })
})

describe('rename or move folder dialog', () => {
  it('quotes a refused destination and reports no move', async () => {
    const sent: Array<{ command_id: string; command: { from: string; to: string } }> = []
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input)
        if (url.includes('/authoritative/folder-moves')) {
          sent.push(JSON.parse(String(init?.body)))
          return jsonResponse({ error: 'immutable sequence already used' }, 409)
        }
        return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
      }),
    )
    const onMoved = vi.fn()
    render(
      <MemoryRouter>
        <QueryClientProvider client={testQueryClient()}>
          <FolderMoveDialog
            path="Work"
            options={folderOptions(tree, { exclude: 'Work' })}
            onClose={() => undefined}
            onMoved={onMoved}
            onRefreshTree={() => undefined}
          />
        </QueryClientProvider>
      </MemoryRouter>,
    )

    const name = screen.getByLabelText(/Name/)
    await userEvent.clear(name)
    await userEvent.type(name, 'Archive')
    await userEvent.click(screen.getByRole('button', { name: 'Rename or move' }))

    await waitFor(() => expect(sent).toHaveLength(1))
    expect(sent[0].command).toEqual({ from: 'Work', to: 'Archive' })
    expect(await screen.findByText(/immutable sequence already used/)).toBeInTheDocument()
    expect(onMoved).not.toHaveBeenCalled()
  })
})
