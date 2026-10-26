import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ActivitySection, originLabel } from '@/features/tasks/task-drawer/activity-section'

describe('activity origin labels', () => {
  it('separates this browser from a genuinely different device', () => {
    expect(originLabel('external:device-a', 'device-a')).toBe('This device')
    expect(originLabel('external:device-b', 'device-a')).toBe('Another device')
    expect(originLabel('external:device-b', null)).toBe('Another device')
    expect(originLabel('system', 'device-a')).toBe('System')
    expect(originLabel('human', 'device-a')).toBe('You')
  })

  it('renders the signed-in device’s own write without “Another device”', () => {
    render(
      <ActivitySection
        loading={false}
        now={1_700_000_000_000}
        sessionDeviceId="device-a"
        entries={[
          {
            id: 'a-1',
            kind: 'task_updated',
            origin: 'external:device-a',
            occurredAtMs: 1_700_000_000_000,
            summary: 'Task planning updated',
          },
          {
            id: 'a-2',
            kind: 'task_updated',
            origin: 'external:device-b',
            occurredAtMs: 1_700_000_000_000,
            summary: 'Task description updated',
          },
        ]}
      />,
    )
    expect(screen.getByText('This device')).toBeInTheDocument()
    expect(screen.getByText('Another device')).toBeInTheDocument()
  })
})
