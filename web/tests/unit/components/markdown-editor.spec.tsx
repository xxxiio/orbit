import { describe, expect, it, vi } from 'vitest'
import { useState } from 'react'
import { fireEvent, render, screen } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { EditGuardProvider, type EditorHandle } from '@/components/forms'
import { MarkdownEditor } from '@/components/markdown'

/** A controlled host: the editor is a draft field, so state lives with the caller. */
function Host({
  initial = '',
  onCommit,
  onSaveShortcut,
  disabled = false,
  guard,
}: {
  initial?: string
  onCommit?: () => void
  onSaveShortcut?: () => void
  disabled?: boolean
  guard?: EditorHandle
}) {
  const [value, setValue] = useState(initial)
  return (
    <MarkdownEditor
      value={value}
      onChange={setValue}
      label="Draft"
      onCommit={onCommit}
      onSaveShortcut={onSaveShortcut}
      disabled={disabled}
      guard={guard}
    />
  )
}

describe('markdown editor', () => {
  it('writes into the draft field and previews the same text', async () => {
    const user = userEvent.setup()
    render(<Host />)

    const field = screen.getByRole('textbox', { name: 'Draft' })
    await user.type(field, '## A heading')
    expect(field).toHaveValue('## A heading')

    await user.click(screen.getByRole('tab', { name: 'Preview' }))
    expect(screen.getByRole('heading', { name: 'A heading' })).toBeInTheDocument()

    await user.click(screen.getByRole('tab', { name: 'Write' }))
    expect(screen.getByRole('textbox', { name: 'Draft' })).toHaveValue('## A heading')
  })

  it('says so when there is nothing to preview', async () => {
    const user = userEvent.setup()
    render(<Host />)

    await user.click(screen.getByRole('tab', { name: 'Preview' }))
    expect(screen.getByText('Nothing to preview yet.')).toBeInTheDocument()
  })

  it('commits on blur and on the save shortcut', () => {
    const onCommit = vi.fn()
    const onSaveShortcut = vi.fn()
    render(<Host initial="text" onCommit={onCommit} onSaveShortcut={onSaveShortcut} />)

    const field = screen.getByRole('textbox', { name: 'Draft' })
    fireEvent.blur(field)
    expect(onCommit).toHaveBeenCalledTimes(1)

    fireEvent.keyDown(field, { key: 'Enter', metaKey: true })
    expect(onSaveShortcut).toHaveBeenCalledTimes(1)
  })

  it('does not save on a plain Enter', () => {
    const onSaveShortcut = vi.fn()
    render(<Host initial="text" onSaveShortcut={onSaveShortcut} />)

    fireEvent.keyDown(screen.getByRole('textbox', { name: 'Draft' }), { key: 'Enter' })
    expect(onSaveShortcut).not.toHaveBeenCalled()
  })

  it('registers the draft with the surrounding dirty guard', () => {
    const flush = vi.fn()
    const guard: EditorHandle = {
      id: 'draft',
      isDirty: () => true,
      flush,
      draftKey: () => 'draft-key',
    }
    const registered: EditorHandle[] = []
    render(
      <EditGuardProvider
        register={(handle) => {
          registered.push(handle)
          return () => {}
        }}
      >
        <Host guard={guard} />
      </EditGuardProvider>,
    )

    expect(registered).toHaveLength(1)
    expect(registered[0].isDirty()).toBe(true)
    registered[0].flush()
    expect(flush).toHaveBeenCalledTimes(1)
  })

  it('keeps the field read-only while a write is in flight', () => {
    render(<Host initial="text" disabled />)
    expect(screen.getByRole('textbox', { name: 'Draft' })).toBeDisabled()
  })
})
