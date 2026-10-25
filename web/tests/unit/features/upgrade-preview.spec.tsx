import { describe, expect, it } from 'vitest'
import { render, screen, within } from '@testing-library/react'
import type { WorkflowUpgradeApplied, WorkflowUpgradePreview } from '@/api/adapters'
import { UpgradeDiff, UpgradeReceipt } from '@/features/workflows/upgrade-preview'

const preview: WorkflowUpgradePreview = {
  schema: 'orbit.blueprint-upgrade-preview.v1',
  projectId: 'p-orbit-development',
  projectRevision: 42,
  current: { workflowId: 'development', version: 2 },
  target: { workflowId: 'development', version: 3 },
  kinds: [
    {
      kind: 'feature',
      added: false,
      removed: false,
      initialStateBefore: 'backlog',
      initialStateAfter: 'backlog',
      statesAdded: ['triage'],
      statesRemoved: [],
      fieldsAdded: [
        { id: 'notes', kind: { type: 'text' }, required: false, default: null },
      ],
      fieldsRemoved: [
        {
          id: 'priority',
          kind: { type: 'choice', options: ['low', 'medium', 'high'] },
          required: true,
          default: 'medium',
        },
      ],
      fieldsChanged: [],
      transitionsAdded: ['triage->ready'],
      transitionsRemoved: [],
      transitionsChanged: [
        {
          transition: 'backlog->ready',
          requiredCapabilityBefore: null,
          requiredCapabilityAfter: 'orbit.release.approve',
          guardsChanged: true,
        },
      ],
      actionsAdded: [
        {
          transition: 'backlog->ready',
          action: 'append_note',
          capability: 'orbit.release.note',
        },
      ],
      actionsRemoved: [],
      requiredCapabilitiesAdded: ['orbit.release.note'],
      requiredCapabilitiesRemoved: [],
    },
  ],
  views: {
    added: ['release'],
    removed: ['agents'],
    changed: [
      {
        view: 'board',
        layoutBefore: 'grid/2',
        layoutAfter: 'vertical/1',
        componentsAdded: ['notes'],
        componentsRemoved: [],
        componentsChanged: ['work_board'],
      },
    ],
  },
  warnings: [
    {
      code: 'field_removed',
      message: 'Field priority is removed from feature.',
      subjects: ['priority'],
    },
  ],
  blockers: [
    {
      code: 'field_removed_with_values',
      message: 'Field priority is removed but 2 items still store a value.',
      subjects: ['wi-orbit-feature-backlog', 'wi-orbit-feature-ready'],
    },
    {
      code: 'state_removed_with_live_items',
      message: 'State review is removed but 1 item still uses it.',
      subjects: ['wi-orbit-feature-review'],
    },
  ],
  fingerprint: 'f'.repeat(64),
}

const viewTitle = (id: string) => (id === 'release' ? 'Release notes' : `View ${id}`)

describe('upgrade diff', () => {
  it('lists every blocker with its code as words and every subject', () => {
    render(<UpgradeDiff preview={preview} viewTitle={viewTitle} />)
    expect(screen.getByText(/Field removed while items still store values/)).toBeInTheDocument()
    expect(
      screen.getByText('wi-orbit-feature-backlog'),
    ).toBeInTheDocument()
    expect(screen.getByText('wi-orbit-feature-ready')).toBeInTheDocument()
    expect(
      screen.getByText(/State removed while items still use it/),
    ).toBeInTheDocument()
    expect(screen.getByText('wi-orbit-feature-review')).toBeInTheDocument()
  })

  it('renders per-kind state, field, transition and action changes', () => {
    render(<UpgradeDiff preview={preview} viewTitle={viewTitle} />)
    const kind = screen.getByText('Feature').closest('[data-upgrade-kind="feature"]') as HTMLElement
    expect(kind).toBeTruthy()
    expect(within(kind).getByText('Triage')).toBeInTheDocument()
    expect(within(kind).getByText('notes')).toBeInTheDocument()
    expect(within(kind).getByText('priority')).toBeInTheDocument()
    expect(within(kind).getByText('Backlog → Ready')).toBeInTheDocument()
    expect(within(kind).getByText(/Capability: none → orbit.release.approve/)).toBeInTheDocument()
    expect(within(kind).getByText(/append_note/)).toBeInTheDocument()
    expect(within(kind).getAllByText(/orbit.release.note/).length).toBeGreaterThan(0)
  })

  it('renders view additions, removals and changes with human titles', () => {
    render(<UpgradeDiff preview={preview} viewTitle={viewTitle} />)
    expect(screen.getByText('Release notes')).toBeInTheDocument()
    expect(screen.getByText('View agents')).toBeInTheDocument()
    expect(screen.getByText(/Layout grid\/2 → vertical\/1/)).toBeInTheDocument()
  })

  it('shows warnings without turning them into blockers', () => {
    render(<UpgradeDiff preview={preview} viewTitle={viewTitle} />)
    expect(screen.getByText(/Field priority is removed from feature/)).toBeInTheDocument()
  })
})

describe('upgrade receipt', () => {
  it('states an already-applied receipt honestly', () => {
    const applied: WorkflowUpgradeApplied = {
      ...preview,
      binding: { projectId: preview.projectId, workflowId: 'development', version: 3 },
      previous: { workflowId: 'development', version: 2 },
      alreadyApplied: true,
    }
    render(<UpgradeReceipt applied={applied} viewTitle={viewTitle} />)
    const receipt = screen.getByText(/Workflow already pinned/).closest('[data-upgrade-receipt]')
    expect(receipt).toBeTruthy()
    expect(screen.getByText(/the server reported that this version was already in place/)).toBeInTheDocument()
    expect(screen.getByText(/fingerprint ffffffff/)).toBeInTheDocument()
  })
})
