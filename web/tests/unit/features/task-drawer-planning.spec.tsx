import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ApiError } from '@/api/errors'
import { PlanningSection } from '@/features/tasks/task-drawer/planning-section'

const stored = {
  estimatedMinutes: null as number | null,
  earliestStartAtMs: null as number | null,
  deadlineAtMs: null as number | null,
}

function renderSection(overrides: Partial<Parameters<typeof PlanningSection>[0]> = {}) {
  const onCommit = vi.fn()
  const view = render(
    <PlanningSection
      task={stored}
      pending={null}
      disabled={false}
      error={null}
      onCommit={onCommit}
      {...overrides}
    />,
  )
  return { onCommit, view }
}

describe('task drawer planning group', () => {
  it('commits an estimate with the other stored planning fields unchanged', async () => {
    const user = userEvent.setup()
    const task = { ...stored, earliestStartAtMs: 1_000, deadlineAtMs: 2_000 }
    const { onCommit } = renderSection({ task })

    const estimate = screen.getByLabelText('Task estimate in minutes')
    await user.clear(estimate)
    await user.type(estimate, '60')
    await user.tab()

    expect(onCommit).toHaveBeenCalledWith('estimate', {
      estimatedMinutes: 60,
      earliestStartAtMs: 1_000,
      deadlineAtMs: 2_000,
    })
  })

  it('clears an estimate with an empty value', async () => {
    const user = userEvent.setup()
    const { onCommit } = renderSection({ task: { ...stored, estimatedMinutes: 60 } })

    const estimate = screen.getByLabelText('Task estimate in minutes')
    await user.clear(estimate)
    await user.tab()

    expect(onCommit).toHaveBeenCalledWith('estimate', {
      estimatedMinutes: null,
      earliestStartAtMs: null,
      deadlineAtMs: null,
    })
  })

  it('commits a deadline as an absolute instant', async () => {
    const user = userEvent.setup()
    const { onCommit } = renderSection()

    const deadline = screen.getByLabelText('Task deadline')
    await user.type(deadline, '2030-01-15T12:30')
    await user.tab()

    expect(onCommit).toHaveBeenCalledWith('deadline', {
      estimatedMinutes: null,
      earliestStartAtMs: null,
      deadlineAtMs: new Date(2030, 0, 15, 12, 30, 0, 0).getTime(),
    })
  })

  it('rolls an invalid draft back to the stored value without sending it', async () => {
    const user = userEvent.setup()
    const { onCommit } = renderSection({ task: { ...stored, estimatedMinutes: 60 } })

    const estimate = screen.getByLabelText('Task estimate in minutes')
    await user.clear(estimate)
    await user.type(estimate, '0')
    await user.tab()

    expect(onCommit).not.toHaveBeenCalled()
    expect(estimate).toHaveValue(60)
    expect(screen.getByRole('alert')).toHaveTextContent(/at least one minute/i)
  })

  it('discards an uncommitted draft when the server refuses the last command', async () => {
    const user = userEvent.setup()
    const { view } = renderSection()

    const estimate = screen.getByLabelText('Task estimate in minutes')
    await user.type(estimate, '90')
    expect(estimate).toHaveValue(90)

    // The refusal does not change the stored record; the draft must not
    // survive it. `error` is the signal the accepted value is not the truth.
    view.rerender(
      <PlanningSection
        task={stored}
        pending={null}
        disabled={false}
        error={new ApiError(409, 'stale server revision')}
        onCommit={vi.fn()}
      />,
    )

    expect(estimate).toHaveValue(null)
    expect(screen.queryByText('90')).not.toBeInTheDocument()
  })
})
