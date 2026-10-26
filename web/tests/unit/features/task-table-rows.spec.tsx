import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClientProvider } from '@tanstack/react-query'
import type { Task } from '@/api/adapters'
import { queryKeys } from '@/api/queries'
import { TaskTable } from '@/features/tasks/task-table'
import { useRowKeys } from '@/hooks'
import { testQueryClient } from '../api/test-utils'

const tasks: Task[] = ['Alpha task', 'Beta task', 'Gamma task'].map((title, index) => ({
  id: `task-${index + 1}`,
  title,
  status: 'ready',
  projectId: null,
  milestoneId: null,
  estimatedMinutes: null,
  earliestStartAtMs: null,
  deadlineAtMs: null,
  description: '',
  priority: null,
  labels: [],
}))

function InboxLike({ onOpen }: { onOpen: (task: Task) => void }) {
  useRowKeys({})

  return (
    <TaskTable
      tasks={tasks}
      projects={[]}
      caption="Unassigned tasks"
      empty={<p>Empty</p>}
      onOpen={onOpen}
      onStatusChange={() => undefined}
      showStatus={false}
      showProject={false}
      showPriority={false}
    />
  )
}

/** The stacked (mobile) copy is in the DOM too, so scope queries to one list. */
function titleButtons(title: string): HTMLElement[] {
  return screen.getAllByRole('button', { name: title })
}

/**
 * The table reads one status edge set per distinct status it shows. Seed the
 * ready edges so the row controls resolve without a network request; these
 * tests only exercise title focus and activation.
 */
function renderInbox(onOpen: (task: Task) => void) {
  const client = testQueryClient()
  client.setQueryData(queryKeys.taskTransitions('ready'), {
    status: 'ready',
    next: ['in_progress'],
  })
  return render(
    <QueryClientProvider client={client}>
      <InboxLike onOpen={onOpen} />
    </QueryClientProvider>,
  )
}

describe('TaskTable row titles', () => {
  it('exposes each row title as a real button with the title as its name', () => {
    renderInbox(() => undefined)

    const [button] = titleButtons('Alpha task')
    expect(button.tagName).toBe('BUTTON')
    expect(button).toHaveAttribute('type', 'button')
    expect(button).toHaveAccessibleName('Alpha task')

    // Focusable: the title is the row's tab stop, not a decorative span.
    button.focus()
    expect(button).toHaveFocus()
    // Still in the tab order (no `tabindex="-1"`) and clickable with the mouse.
    expect(button).not.toHaveAttribute('tabindex', '-1')
  })

  it('opens the task on Enter and on Space', async () => {
    const onOpen = vi.fn()
    renderInbox(onOpen)

    const [button] = titleButtons('Alpha task')
    button.focus()

    await userEvent.keyboard('{Enter}')
    expect(onOpen).toHaveBeenLastCalledWith(tasks[0])

    await userEvent.keyboard(' ')
    expect(onOpen).toHaveBeenLastCalledWith(tasks[0])
    expect(onOpen).toHaveBeenCalledTimes(2)
  })

  it('moves focus across rows with j/k and activates with Enter', async () => {
    const onOpen = vi.fn()
    renderInbox(onOpen)

    const [first] = titleButtons('Alpha task')
    const [second] = titleButtons('Beta task')
    first.focus()

    await userEvent.keyboard('j')
    expect(second).toHaveFocus()

    await userEvent.keyboard('k')
    expect(first).toHaveFocus()

    await userEvent.keyboard('j')
    await userEvent.keyboard('{Enter}')
    expect(onOpen).toHaveBeenLastCalledWith(tasks[1])
  })
})
