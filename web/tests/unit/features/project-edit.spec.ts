import { describe, expect, it } from 'vitest'
import type { Project, ProjectStatusOption } from '@/api/adapters'
import { planProjectUpdate, projectNameError, projectStatusChoices } from '@/features/projects/project-edit'

const project: Project = { id: 'orbit', name: 'Orbit', status: 'active' }

const statuses: ProjectStatusOption[] = [
  { status: 'proposed', next: ['active', 'cancelled'] },
  { status: 'active', next: ['on_hold', 'completed'] },
  { status: 'on_hold', next: ['active', 'cancelled'] },
]

describe('project update planning', () => {
  it('sends only the name when only the name changed', () => {
    expect(planProjectUpdate(project, { name: ' Orbit Web ', status: 'active' })).toEqual({
      name: 'Orbit Web',
    })
  })

  it('sends an empty name on a status-only change, meaning "keep the name"', () => {
    expect(planProjectUpdate(project, { name: 'Orbit', status: 'on_hold' })).toEqual({
      name: '',
      status: 'on_hold',
    })
  })

  it('sends both when both changed', () => {
    expect(planProjectUpdate(project, { name: 'Orbit Web', status: 'on_hold' })).toEqual({
      name: 'Orbit Web',
      status: 'on_hold',
    })
  })

  it('returns null for an unchanged draft or an empty name', () => {
    expect(planProjectUpdate(project, { name: 'Orbit', status: 'active' })).toBeNull()
    expect(planProjectUpdate(project, { name: '   ', status: 'active' })).toBeNull()
    expect(projectNameError('   ')).toBe('A project needs a name.')
    expect(projectNameError('Orbit')).toBeNull()
  })
})

describe('project status choices', () => {
  it('offers the stored status plus only the server-reported targets', () => {
    expect(projectStatusChoices(statuses, 'active')).toEqual({
      choices: ['active', 'on_hold', 'completed'],
      read: true,
    })
  })

  it('does not add a guessed list when the server did not report the status', () => {
    expect(projectStatusChoices(statuses, 'archived')).toEqual({
      choices: ['archived'],
      read: false,
    })
    expect(projectStatusChoices(undefined, 'active')).toEqual({
      choices: ['active'],
      read: false,
    })
  })

  it('keeps a terminal status as the only choice when it has no edges', () => {
    expect(
      projectStatusChoices([{ status: 'archived', next: [] }], 'archived'),
    ).toEqual({ choices: ['archived'], read: true })
  })
})
