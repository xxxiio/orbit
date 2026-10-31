import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useState } from 'react'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter, useLocation } from 'react-router-dom'
import type { MilestoneRecord } from '@/api/contracts'
import { CreateTaskDialog, type CreateTaskDialogProps } from '@/features/tasks/create-task-dialog'
import { ToastProvider } from '@/components/ui/toast'
import { jsonResponse, snapshotAt, taskRecord, testQueryClient } from '../api/test-utils'

/**
 * The full create dialog collects one draft and sends it as one `create`
 * command. These tests pin the parts the contract calls out: the whole payload
 * travels together (nothing is written after the task exists), a retry after an
 * uncertain failure reuses the ids, and a draft is never discarded silently.
 */

const milestone = (overrides: Partial<MilestoneRecord> = {}): MilestoneRecord => ({
  id: 'm-1',
  project_id: 'p-1',
  name: 'Platform launch',
  description: '',
  status: 'planned',
  priority: null,
  owner: null,
  start_at_ms: null,
  target_at_ms: null,
  created_at_ms: 0,
  updated_at_ms: 0,
  task_ids: [],
  work_item_ids: [],
  progress: { total: 0, completed: 0, blocked: 0, percent: 0 },
  activities: [],
  ...overrides,
})

const existingTask = taskRecord({
  id: 't-existing',
  title: 'Finish the schema',
  status: 'planned',
  project_id: 'p-1',
})

interface SentCommand {
  command_id: string
  expected_revision: number
  action: Record<string, unknown> & { task_id: string }
}

function stubApi(options: { failFirstCreate?: boolean } = {}) {
  const sent: SentCommand[] = []
  let creates = 0
  const snapshot = snapshotAt(1, {
    projects: [{ id: 'p-1', name: 'Orbit', status: 'active' }],
    tasks: [existingTask],
  })

  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input)
      if (url.endsWith('/authoritative/snapshot')) {
        return jsonResponse(snapshot)
      }
      if (url.endsWith('/authoritative/milestones')) {
        return jsonResponse({ revision: 1, milestones: [milestone()] })
      }
      if (url.endsWith('/authoritative/task-commands')) {
        const command = JSON.parse(String(init?.body)) as SentCommand
        sent.push(command)
        creates += 1
        if (options.failFirstCreate && creates === 1) {
          throw new TypeError('network down')
        }
        return jsonResponse({
          revision: 2,
          duplicate: false,
          task: taskRecord({
            id: command.action.task_id,
            title: String(command.action.title),
            status: 'inbox',
            project_id: 'p-1',
            milestone_id: 'm-1',
            description: 'Ship it carefully.\n\n- one\n- two',
            priority: 'high',
            estimated_minutes: 45,
            deadline_at_ms: command.action.deadline_at_ms as number,
          }),
        })
      }
      return jsonResponse({ error: `unexpected fetch ${url}` }, 404)
    }),
  )

  return { sent }
}

function LocationProbe() {
  const location = useLocation()
  return <div data-testid="location">{location.search}</div>
}

function renderDialog(props: Partial<CreateTaskDialogProps> = {}) {
  const onOpenChange = vi.fn()
  const view = render(
    <MemoryRouter initialEntries={['/inbox']}>
      <QueryClientProvider client={testQueryClient()}>
        <ToastProvider>
          <LocationProbe />
          <CreateTaskDialog open onOpenChange={onOpenChange} {...props} />
        </ToastProvider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
  return { onOpenChange, ...view }
}

async function choose(user: ReturnType<typeof userEvent.setup>, name: string, option: string) {
  const trigger = screen.getByRole('combobox', { name })
  // The milestone list arrives asynchronously; a disabled trigger swallows the
  // click rather than opening an empty menu.
  await waitFor(() => expect(trigger).toBeEnabled())
  await user.click(trigger)
  await user.click(await screen.findByRole('option', { name: option }))
}

/** A stateful parent, so a test can close the dialog and genuinely reopen it. */
function DialogHarness(props: Partial<CreateTaskDialogProps>) {
  const [open, setOpen] = useState(true)
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Reopen
      </button>
      <CreateTaskDialog open={open} onOpenChange={setOpen} {...props} />
    </>
  )
}

function renderDialogHarness(props: Partial<CreateTaskDialogProps> = {}) {
  render(
    <MemoryRouter initialEntries={['/inbox']}>
      <QueryClientProvider client={testQueryClient()}>
        <ToastProvider>
          <DialogHarness {...props} />
        </ToastProvider>
      </QueryClientProvider>
    </MemoryRouter>,
  )
}

beforeEach(() => {
  // Radix Select uses pointer capture, which jsdom does not implement.
  if (!HTMLElement.prototype.hasPointerCapture) {
    Object.defineProperty(HTMLElement.prototype, 'hasPointerCapture', {
      writable: true,
      value: () => false,
    })
  }
  if (!HTMLElement.prototype.releasePointerCapture) {
    Object.defineProperty(HTMLElement.prototype, 'releasePointerCapture', {
      writable: true,
      value: () => undefined,
    })
  }
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('create task dialog', () => {
  it('sends every configured field in one create command', async () => {
    const { sent } = stubApi()
    const { onOpenChange } = renderDialog({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Title/), 'Ship the release')
    await user.type(
      screen.getByLabelText('Description'),
      'Ship it carefully.\n\n- one\n- two',
    )
    await choose(user, 'Task priority', 'High')
    await choose(user, 'Task milestone', 'Platform launch')

    fireEvent.change(screen.getByLabelText('Deadline'), {
      target: { value: '2026-11-05T17:30' },
    })

    await user.click(screen.getByRole('button', { name: /Advanced/ }))
    await user.type(screen.getByLabelText('Estimate (minutes)'), '45')

    await choose(user, 'Add a prerequisite', 'Finish the schema')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    await user.click(screen.getByRole('button', { name: 'Add criterion' }))
    await user.type(screen.getByLabelText('Acceptance criterion 1'), 'First check')
    await user.click(screen.getByRole('button', { name: 'Add criterion' }))
    await user.type(screen.getByLabelText('Acceptance criterion 2'), 'Second check')

    await user.click(screen.getByRole('button', { name: 'Create task' }))

    await waitFor(() => expect(sent).toHaveLength(1))
    const command = sent[0]
    expect(command.action).toEqual({
      type: 'create',
      task_id: expect.any(String),
      title: 'Ship the release',
      project_id: 'p-1',
      milestone_id: 'm-1',
      description: 'Ship it carefully.\n\n- one\n- two',
      status: 'inbox',
      priority: 'high',
      estimated_minutes: 45,
      deadline_at_ms: new Date(2026, 10, 5, 17, 30).getTime(),
      acceptance_criteria: ['First check', 'Second check'],
      prerequisite_ids: ['t-existing'],
    })
    expect(command.command_id).toEqual(expect.any(String))

    // One write only: the response already carries every configured field.
    expect(vi.mocked(fetch).mock.calls.filter(([url]) => String(url).endsWith('/task-commands'))).toHaveLength(1)

    const toast = await screen.findByText('Task created')
    expect(toast).toBeInTheDocument()
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))
    await user.click(screen.getByRole('button', { name: 'Open task' }))
    await waitFor(() => expect(screen.getByTestId('location')).toHaveTextContent('task='))
  }, 20_000)

  it('reuses the command id and task id when an uncertain create is retried', async () => {
    const { sent } = stubApi({ failFirstCreate: true })
    const { onOpenChange } = renderDialog({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Title/), 'Retry me')
    await user.click(screen.getByRole('button', { name: 'Create task' }))
    await screen.findByText(/may or may not have been saved/i)

    await user.click(screen.getByRole('button', { name: 'Create task' }))
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false))

    expect(sent).toHaveLength(2)
    expect(sent[1].command_id).toBe(sent[0].command_id)
    expect(sent[1].action.task_id).toBe(sent[0].action.task_id)
    expect(sent[1].action.title).toBe('Retry me')
  })

  it('keeps an uncertain attempt across close/reopen and warns about a duplicate', async () => {
    const { sent } = stubApi({ failFirstCreate: true })
    renderDialogHarness({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Title/), 'Retry after close')
    await user.click(screen.getByRole('button', { name: 'Create task' }))
    await screen.findByText(/may or may not have been saved/i)

    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    const confirm = screen.getByRole('group', { name: 'Discard this draft' })
    expect(within(confirm).getByText(/may already have created this task/i)).toBeInTheDocument()
    await user.click(within(confirm).getByRole('button', { name: 'Close anyway' }))
    await waitFor(() => expect(screen.queryByLabelText(/Title/)).toBeNull())

    await user.click(screen.getByRole('button', { name: 'Reopen' }))
    await waitFor(() =>
      expect(screen.getByLabelText(/Title/)).toHaveValue('Retry after close'),
    )
    expect(screen.getByText(/may or may not have been saved/i)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Create task' }))
    await waitFor(() => expect(sent).toHaveLength(2))
    expect(sent[1].command_id).toBe(sent[0].command_id)
    expect(sent[1].action.task_id).toBe(sent[0].action.task_id)
  })

  it('enforces the estimate window and acceptance-criterion bound', async () => {
    const { sent } = stubApi()
    renderDialog({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Title/), 'Bounded fields')
    await user.click(screen.getByRole('button', { name: /Advanced/ }))

    fireEvent.change(screen.getByLabelText('Estimate (minutes)'), { target: { value: '0' } })
    await user.click(screen.getByRole('button', { name: 'Create task' }))
    expect(screen.getByText('An estimate must be at least one minute.')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Estimate (minutes)'), {
      target: { value: '525601' },
    })
    await user.click(screen.getByRole('button', { name: 'Create task' }))
    expect(screen.getByText('Keep the estimate at 525600 minutes or less.')).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Estimate (minutes)'), {
      target: { value: '525600' },
    })
    await user.click(screen.getByRole('button', { name: 'Add criterion' }))
    fireEvent.change(screen.getByLabelText('Acceptance criterion 1'), {
      target: { value: 'x'.repeat(2001) },
    })
    await user.click(screen.getByRole('button', { name: 'Create task' }))
    expect(
      screen.getByText('Keep each acceptance criterion at 2000 characters or less.'),
    ).toBeInTheDocument()
    expect(sent).toHaveLength(0)
  })

  it('stops adding acceptance criteria at the domain maximum', async () => {
    stubApi()
    renderDialog({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: /Advanced/ }))
    const add = screen.getByRole('button', { name: 'Add criterion' })
    for (let index = 0; index < 100; index += 1) fireEvent.click(add)

    expect(
      screen.getAllByRole('textbox', { name: /^Acceptance criterion \d+$/ }),
    ).toHaveLength(100)
    expect(add).toBeDisabled()
  })

  it('shows field-level errors and sends nothing', async () => {
    const { sent } = stubApi()
    renderDialog({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Create task' }))
    expect(screen.getByText('Enter a title for this task.')).toBeInTheDocument()

    await user.type(screen.getByLabelText(/Title/), 'Has a title')
    await user.click(screen.getByRole('button', { name: /Advanced/ }))
    fireEvent.change(screen.getByLabelText('Estimate (minutes)'), { target: { value: '-5' } })
    await user.click(screen.getByRole('button', { name: 'Create task' }))

    expect(screen.getByText('Use whole minutes.')).toBeInTheDocument()
    expect(sent).toHaveLength(0)
  })

  it('preselects a milestone passed by a milestone entry point', async () => {
    stubApi()
    renderDialog({ defaultProjectId: 'p-1', defaultMilestoneId: 'm-1' })
    await waitFor(() =>
      expect(screen.getByRole('combobox', { name: 'Task milestone' })).toHaveTextContent(
        'Platform launch',
      ),
    )
  })

  it('closes an untouched draft directly', async () => {
    stubApi()
    const { onOpenChange } = renderDialog({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })

  it('asks before discarding typed input', async () => {
    stubApi()
    const { onOpenChange } = renderDialog({ defaultProjectId: 'p-1' })
    const user = userEvent.setup()

    await user.type(screen.getByLabelText(/Title/), 'Not sent yet')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(onOpenChange).not.toHaveBeenCalled()
    const confirm = screen.getByRole('group', { name: 'Discard this draft' })
    await user.click(within(confirm).getByRole('button', { name: 'Keep editing' }))
    expect(screen.queryByRole('group', { name: 'Discard this draft' })).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    await user.click(
      within(screen.getByRole('group', { name: 'Discard this draft' })).getByRole('button', {
        name: 'Discard draft',
      }),
    )
    expect(onOpenChange).toHaveBeenCalledWith(false)
  })
})
