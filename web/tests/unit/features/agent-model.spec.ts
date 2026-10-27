import { describe, expect, it } from 'vitest'
import {
  AGENT_TERMINAL_STATES,
  agentStateLabel,
  claimStateCopy,
  isAgentJobTerminal,
  nextActionExplanation,
  nextActionLabel,
} from '@/features/agents/agent-model'

describe('agent job presentation model', () => {
  it('offers resume only for the terminal states the server accepts', () => {
    expect(AGENT_TERMINAL_STATES).toEqual(['completed', 'failed', 'interrupted'])
    expect(isAgentJobTerminal('completed')).toBe(true)
    expect(isAgentJobTerminal('failed')).toBe(true)
    expect(isAgentJobTerminal('interrupted')).toBe(true)
    expect(isAgentJobTerminal('queued')).toBe(false)
    expect(isAgentJobTerminal('running')).toBe(false)
    expect(isAgentJobTerminal('blocked')).toBe(false)
  })

  it('still names a state or action this build does not know', () => {
    expect(agentStateLabel('paused_by_operator')).toBe('paused by operator')
    expect(nextActionLabel('ask_a_human')).toBe('ask a human')
  })

  it('does not let a job awaiting its executor read as "nothing to do"', () => {
    expect(nextActionLabel('await_executor')).toBe('Awaiting an executor')
    expect(nextActionExplanation('await_executor')).toContain('No executor has claimed')
    expect(nextActionExplanation('await_executor')).toContain('stays queued')
  })
})

describe('claim state copy (C13)', () => {
  it('reads a null claim as waiting for an executor, not as progress', () => {
    const copy = claimStateCopy({ reported: true, claimState: null })
    expect(copy.kind).toBe('none')
    expect(copy.label).toBe('Waiting for an executor')
    expect(copy.detail).toContain('stays queued')
    expect(copy.blocksRelaunch).toBe(false)
  })

  it('keeps "not reported" apart from "not claimed" on an older server', () => {
    const copy = claimStateCopy({ reported: false, claimState: null })
    expect(copy.kind).toBe('unreported')
    expect(copy.label).toBe('Claim state not reported')
    expect(copy.blocksRelaunch).toBe(false)
  })

  it('explains a claimed and a confirmed launch differently', () => {
    const claimed = claimStateCopy({ reported: true, claimState: 'claimed' })
    expect(claimed.kind).toBe('claimed')
    expect(claimed.detail).toContain('not confirmed yet')

    const confirmed = claimStateCopy({ reported: true, claimState: 'confirmed' })
    expect(confirmed.kind).toBe('confirmed')
    expect(confirmed.detail).toContain('executor’s own reports')
  })

  it('refuses a relaunch for an ambiguous claim', () => {
    const copy = claimStateCopy({ reported: true, claimState: 'ambiguous' })
    expect(copy.kind).toBe('ambiguous')
    expect(copy.blocksRelaunch).toBe(true)
    expect(copy.tone).toBe('danger')
    expect(copy.detail).toContain('Do not relaunch')
    expect(copy.detail).toContain('second agent')
  })

  it('treats a claim state this build does not know as unsafe to relaunch', () => {
    const copy = claimStateCopy({ reported: true, claimState: 'reconciling' })
    expect(copy.kind).toBe('unrecognised')
    expect(copy.blocksRelaunch).toBe(true)
  })
})
