import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Button } from '@/components/ui/button'

describe('Button', () => {
  it('reports a pending action as busy and refuses activation', async () => {
    const onClick = vi.fn()
    render(
      <Button loading onClick={onClick}>
        Saving changes
      </Button>,
    )

    const button = screen.getByRole('button', { name: /saving changes/i })
    expect(button).toBeDisabled()
    expect(button).toHaveAttribute('aria-busy', 'true')

    await userEvent.click(button)
    expect(onClick).not.toHaveBeenCalled()
  })
})
