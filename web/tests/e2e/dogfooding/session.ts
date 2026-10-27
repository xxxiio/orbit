/**
 * The dogfooding session driver.
 *
 * It is a *person-shaped* sequence (capture, find, prioritise, plan, progress,
 * unblock, write knowledge, review an agent job, run a capability, complete and
 * review a milestone) rather than an acceptance spec: a step that cannot be
 * completed is recorded as a finding and the session continues with the next
 * plausible action. Failures are evidence, not a test failure — the spec that
 * drives this asserts only that the run *recorded* something honest.
 *
 * Screenshots go to `.slim/deepwork/m32-web-completeness/i5-dogfood/captures/`
 * next to a caption file written by `writeSessionRecords`.
 */
import type { Page } from '@playwright/test'

import type { DogfoodWorkspace } from './workspace'

export interface StepRecord {
  id: string
  phase: string
  action: string
  expected: string
  observed: string
  friction: string[]
  status: 'ok' | 'failed' | 'skipped'
  durationMs: number
  screenshot: string | null
  /** Values later steps depend on (ids, names), kept for the written log. */
  data?: Record<string, string>
}

export interface SessionReport {
  label: 'automated interactive dogfooding'
  runIndex: number
  resumed: boolean
  startedAtMs: number
  finishedAtMs: number
  steps: StepRecord[]
  browserErrors: string[]
  failedResponses: Array<{ method: string; url: string; status: number }>
  slowRequests: Array<{ method: string; url: string; status: number; ms: number }>
  notes: string[]
}

export interface DogfoodContext {
  workspace: DogfoodWorkspace
  runIndex: number
  /** A per-run suffix so a re-run adds new work instead of colliding. */
  marker: string
  /** Ids captured by earlier steps. */
  ids: Record<string, string>
  /** A per-run scratch space for step authors. */
  scratch: Record<string, unknown>
}

const CAPTURES = 'captures'

/**
 * Playwright assertions inside a dogfooding step must not abort the session:
 * `expect.soft` collects, `try/catch` around one step keeps the rest running.
 * Every step is wrapped by `runStep`.
 */
export async function runStep(
  page: Page,
  capturesDir: string,
  step: Omit<StepRecord, 'status' | 'durationMs' | 'screenshot' | 'observed' | 'friction'> & {
    friction?: string[]
    run: () => Promise<{ observed?: string; friction?: string[]; data?: Record<string, string> } | void>
  },
): Promise<StepRecord> {
  const started = Date.now()
  const record: StepRecord = {
    id: step.id,
    phase: step.phase,
    action: step.action,
    expected: step.expected,
    observed: '',
    friction: [...(step.friction ?? [])],
    status: 'ok',
    durationMs: 0,
    screenshot: null,
  }
  try {
    const result = await step.run()
    if (result?.observed) record.observed = result.observed
    if (result?.friction) record.friction.push(...result.friction)
    if (result?.data) record.data = result.data
  } catch (error) {
    record.status = 'failed'
    record.observed = error instanceof Error ? error.message : String(error)
  }
  record.durationMs = Date.now() - started
  const shot = `${capturesDir}/${step.id}.png`
  try {
    await page.screenshot({ path: shot, fullPage: false })
    record.screenshot = `${CAPTURES}/${step.id}.png`
  } catch {
    record.screenshot = null
  }
  return record
}

export function captureTotals(report: SessionReport): { steps: number; failures: number } {
  return {
    steps: report.steps.length,
    failures: report.steps.filter((step) => step.status === 'failed').length,
  }
}
