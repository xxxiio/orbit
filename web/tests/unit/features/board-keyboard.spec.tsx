import { describe, expect, it } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { useBoardKeyboard, type BoardGroup } from '@/components/surface'

interface Row {
  id: string
}

function groups(ids: string[][]): BoardGroup<Row>[] {
  return ids.map((rows, index) => ({
    key: `column-${index}`,
    rows: rows.map((id) => ({ id })),
  }))
}

describe('board keyboard focus', () => {
  it('moves the tab stop to the first card when the focused card disappears', async () => {
    const { result, rerender } = renderHook(
      ({ columns }: { columns: BoardGroup<Row>[] }) => useBoardKeyboard<Row>(columns),
      { initialProps: { columns: groups([['a-1', 'a-2'], ['b-1']]) } },
    )

    act(() => result.current.setFocus('a-2'))
    expect(result.current.tabStop).toBe('a-2')

    // A move or a refresh removed the focused card: exactly one card must keep
    // `tabIndex=0`, so the cursor falls back to the first card.
    rerender({ columns: groups([['a-1'], ['b-1']]) })
    await waitFor(() => expect(result.current.tabStop).toBe('a-1'))
    expect(result.current.focus).toBe('a-1')
  })

  it('leaves the default tab stop alone while the focused card is present', async () => {
    const { result, rerender } = renderHook(
      ({ columns }: { columns: BoardGroup<Row>[] }) => useBoardKeyboard<Row>(columns),
      { initialProps: { columns: groups([['a-1', 'a-2'], ['b-1']]) } },
    )

    act(() => result.current.setFocus('b-1'))
    rerender({ columns: groups([['a-1', 'a-2'], ['b-1']]) })
    await waitFor(() => expect(result.current.tabStop).toBe('b-1'))
  })
})
