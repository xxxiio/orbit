import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { DataTable, type DataTableColumn } from '@/components/ui/data-table'

interface Row {
  id: string
  name: string
}

const columns: Array<DataTableColumn<Row>> = [
  { key: 'name', header: 'Name', render: (row) => row.name },
]

const rows: Row[] = [
  { id: '1', name: 'Alpha' },
  { id: '2', name: 'Beta' },
  { id: '3', name: 'Gamma' },
]

describe('DataTable', () => {
  it('moves row focus with arrows and activates the focused row', async () => {
    const onRowActivate = vi.fn()
    render(
      <DataTable
        caption="Projects"
        columns={columns}
        rows={rows}
        getRowKey={(row) => row.id}
        onRowActivate={onRowActivate}
      />,
    )

    const first = screen.getByText('Alpha').closest('tr')
    const second = screen.getByText('Beta').closest('tr')
    const third = screen.getByText('Gamma').closest('tr')

    first?.focus()
    expect(first).toHaveFocus()

    await userEvent.keyboard('{ArrowDown}')
    expect(second).toHaveFocus()

    await userEvent.keyboard('{End}')
    expect(third).toHaveFocus()

    await userEvent.keyboard('{Enter}')
    expect(onRowActivate).toHaveBeenCalledWith(rows[2])
  })
})
