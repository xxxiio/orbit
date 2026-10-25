import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'

function renderEmptyState(ui: React.ReactNode) {
  return render(<MemoryRouter>{ui}</MemoryRouter>)
}

describe('EmptyState actions', () => {
  it('renders a primary and a secondary action side by side', () => {
    renderEmptyState(
      <EmptyState
        title="Nothing planned yet"
        description="Capture your first task."
        action={<Button onClick={() => undefined}>Create the first task</Button>}
        secondaryAction={
          <Button asChild variant="ghost" size="sm">
            <a href="/projects">Open projects</a>
          </Button>
        }
      />,
    )

    expect(screen.getByRole('button', { name: 'Create the first task' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Open projects' })).toBeInTheDocument()
  })

  /**
   * Regression: Home's empty workspace threw
   * `Slot failed to slot onto its children. Expected a single React element
   * child or Slottable` and the root error boundary replaced the screen. An
   * action node is allowed to hold several children; the composition has to
   * absorb that instead of asking every caller to wrap it.
   */
  it('accepts an action node that holds several children', async () => {
    const onPrimary = vi.fn()
    const onSecondary = vi.fn()

    renderEmptyState(
      <EmptyState
        title="Nothing planned yet"
        action={
          <>
            <Button onClick={onPrimary}>Create the first task</Button>
            <Button variant="secondary" onClick={onSecondary}>
              Import tasks
            </Button>
          </>
        }
        secondaryAction={
          <Button asChild variant="ghost" size="sm">
            <a href="/projects">Open projects</a>
          </Button>
        }
      />,
    )

    await userEvent.click(screen.getByRole('button', { name: 'Create the first task' }))
    expect(onPrimary).toHaveBeenCalledTimes(1)
    await userEvent.click(screen.getByRole('button', { name: 'Import tasks' }))
    expect(onSecondary).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('link', { name: 'Open projects' })).toBeInTheDocument()
  })

  it('slots an asChild action onto its element without dropping the link behaviour', () => {
    renderEmptyState(
      <EmptyState
        title="No projects yet"
        action={
          <Button asChild variant="secondary" size="sm">
            <a href="/projects">
              <span aria-hidden="true">+</span> Open projects
            </a>
          </Button>
        }
      />,
    )

    const link = screen.getByRole('link', { name: 'Open projects' })
    expect(link.tagName).toBe('A')
    expect(link).toHaveAttribute('href', '/projects')
    // The slotted element stays a real, focusable link rather than becoming a
    // decorative wrapper around the button's styling.
    link.focus()
    expect(link).toHaveFocus()
  })
})

describe('Button asChild', () => {
  it('keeps working when the slotted children are a fragment', () => {
    render(
      <MemoryRouter>
        <Button asChild>
          <>
            <a href="/projects">Open projects</a>
          </>
        </Button>
      </MemoryRouter>,
    )

    expect(screen.getByRole('link', { name: 'Open projects' })).toBeInTheDocument()
  })

  it('does not throw when the slotted children are several elements', () => {
    expect(() =>
      render(
        <MemoryRouter>
          <Button asChild>
            <>
              <span>One</span>
              <span>Two</span>
            </>
          </Button>
        </MemoryRouter>,
      ),
    ).not.toThrow()
    expect(screen.getByText('One')).toBeInTheDocument()
    expect(screen.getByText('Two')).toBeInTheDocument()
  })
})
