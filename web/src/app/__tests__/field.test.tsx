import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'

describe('Field', () => {
  it('connects label, hint, and error to the control for assistive technology', () => {
    render(
      <Field label="Device ID" hint="The identifier for this browser." error="Enter the device ID.">
        <Input />
      </Field>,
    )

    const input = screen.getByLabelText('Device ID')
    expect(input).toHaveAttribute('aria-invalid', 'true')

    const describedBy = input.getAttribute('aria-describedby')
    expect(describedBy).toBeTruthy()
    const ids = describedBy?.split(' ') ?? []
    expect(ids).toHaveLength(2)
    expect(document.getElementById(ids[0])).toHaveTextContent('The identifier for this browser.')
    expect(document.getElementById(ids[1])).toHaveTextContent('Enter the device ID.')

    expect(screen.getByRole('alert')).toHaveTextContent('Enter the device ID.')
  })
})
