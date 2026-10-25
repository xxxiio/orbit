import { describe, expect, it } from 'vitest'
import {
  AGENT_TERMINAL_STATES,
  agentStateLabel,
  isAgentJobTerminal,
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
})
