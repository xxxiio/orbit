import { describe, expect, it, vi } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { AcceptanceCriterion } from '@/api/adapters'
import { CriteriaSection } from '@/features/tasks/task-drawer/criteria-section'
import type { TaskCommand } from '@/features/tasks/task-drawer/task-command'

/**
 * A checklist draft is not the previous save's to clear.
 *
 * The first criterion is still being stored when the person starts typing the
 * next one. When that save completes it must clear only the text it submitted;
 * a newer draft has to survive, or the person silently loses what they typed.
 */

const command: TaskCommand = {
  isPending: false,
  error: null,
  onRefresh: undefined,
} as unknown as TaskCommand

function renderSection(onAdd: (text: string) => Promise<boolean>) {
  const criteria: AcceptanceCriterion[] = []
  return render(
    <CriteriaSection
      taskId="t-1"
      criteria={criteria}
      pending={false}
      error={null}
      command={command}
      onAdd={onAdd}
      onChangeText={vi.fn()}
      onToggle={vi.fn()}
      onReorder={vi.fn()}
      onRemove={vi.fn()}
    />,
  )
}

describe('acceptance criteria', () => {
  it('keeps a draft typed while the previous criterion is still saving', async () => {
    const user = userEvent.setup()
    let finishFirst: (value: boolean) => void = () => undefined
    const onAdd = vi
      .fn<(text: string) => Promise<boolean>>()
      .mockImplementationOnce(() => new Promise<boolean>((resolve) => {
        finishFirst = resolve
      }))
      .mockResolvedValue(true)

    renderSection(onAdd)

    const input = screen.getByLabelText('New acceptance criterion') as HTMLInputElement
    await user.type(input, 'first criterion{Enter}')
    expect(onAdd).toHaveBeenCalledWith('first criterion')
    // The command is still in flight; the person replaces the text with the next
    // criterion before the answer arrives.
    await user.clear(input)
    await user.type(input, 'second criterion')

    await act(async () => {
      finishFirst(true)
    })

    expect(
      input.value,
      'the newer draft must not be cleared by the earlier save',
    ).toBe('second criterion')
  })
})
