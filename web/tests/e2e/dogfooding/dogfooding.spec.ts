/**
 * Automated interactive dogfooding — one simulated day of development
 * management on a **persistent** workspace.
 *
 * This is not an acceptance suite and it is not a person: it is a scripted
 * Playwright session whose *steps* are the product's own screens, run against a
 * database that survives between runs. It records what happened, what was
 * expected and where the friction was in
 * `.slim/deepwork/m32-web-completeness/i5-dogfood/`.
 *
 * Opt-in (`ORBIT_DOGFOOD=1`): a normal suite run must not append a day of work
 * to the dogfooding database, and the database must not decide whether the suite
 * is green. The command is in the deliverable and in the folder README.
 */
import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import { test } from '@playwright/test'

import { REPO_ROOT } from '../harness/server'
import { phases } from './phases'
import { ensureSeeded } from './seed'
import {
  captureTotals,
  type DogfoodContext,
  type SessionReport,
  type StepRecord,
} from './session'
import { openWorkspace, workspaceCommit } from './workspace'

const ARTIFACT_DIR = join(REPO_ROOT, '.slim', 'deepwork', 'm32-web-completeness', 'i5-dogfood')
const CAPTURES_DIR = join(ARTIFACT_DIR, 'captures')

test.describe('automated interactive dogfooding', () => {
  test.skip(
    process.env.ORBIT_DOGFOOD !== '1',
    'opt-in persistent session — run with ORBIT_DOGFOOD=1 (see tests/e2e/dogfooding/README.md)',
  )

  test('one day of development management on a persistent workspace', async ({
    page,
    context,
  }, testInfo) => {
    test.setTimeout(20 * 60_000)
    await mkdir(CAPTURES_DIR, { recursive: true })

    const workspace = await openWorkspace()
    const steps: StepRecord[] = []
    const browserErrors: string[] = []
    const failedResponses: SessionReport['failedResponses'] = []
    const slowRequests: SessionReport['slowRequests'] = []
    const startedAtMs = Date.now()

    try {
      const seeded = await ensureSeeded(workspace)
      const runIndex = workspace.state.runs.length + 1
      const marker = `DF${runIndex}`

      page.on('pageerror', (error) => browserErrors.push(`pageerror: ${error.message}`))
      page.on('console', (message) => {
        if (message.type() === 'error') browserErrors.push(`console: ${message.text()}`)
      })
      page.on('response', (response) => {
        const ms = Math.round(response.request().timing().responseEnd)
        if (ms >= 1_000) {
          slowRequests.push({
            method: response.request().method(),
            url: response.url().replace(workspace.base, ''),
            status: response.status(),
            ms,
          })
        }
        if (response.status() >= 400) {
          failedResponses.push({
            method: response.request().method(),
            url: response.url().replace(workspace.base, ''),
            status: response.status(),
          })
        }
        if (response.status() >= 500) {
          browserErrors.push(
            `http ${response.status()}: ${response.request().method()} ${response.url()}`,
          )
        }
      })

      await context.tracing.start({ screenshots: true, snapshots: true, sources: true })

      const ctx: DogfoodContext = {
        workspace,
        runIndex,
        marker,
        ids: {},
        scratch: { capturesDir: CAPTURES_DIR },
      }
      const only = (process.env.ORBIT_DOGFOOD_ONLY ?? '')
        .split(',')
        .map((entry) => entry.trim())
        .filter(Boolean)

      for (const phase of phases()) {
        if (only.length > 0 && !only.includes(phase.id)) continue
        try {
          await phase.run(page, ctx, (step) => {
            steps.push(step)
          })
        } catch (error) {
          steps.push({
            id: `${phase.id}-aborted`,
            phase: phase.id,
            action: phase.title,
            expected: 'The phase runs to completion.',
            observed: error instanceof Error ? error.message : String(error),
            friction: ['The phase aborted before its recorded steps finished.'],
            status: 'failed',
            durationMs: 0,
            screenshot: null,
          })
        }
      }

      const finishedAtMs = Date.now()
      const traceName = `trace-run-${runIndex}.zip`
      await context.tracing.stop({ path: join(CAPTURES_DIR, traceName) })

      const totals = captureTotals({ steps } as SessionReport)
      const report: SessionReport = {
        label: 'automated interactive dogfooding',
        runIndex,
        resumed: workspace.resumed,
        startedAtMs,
        finishedAtMs,
        steps,
        browserErrors,
        failedResponses,
        slowRequests,
        notes: [
          seeded.seeded
            ? 'This run seeded the bulk fixture through the public API.'
            : 'This run resumed an already-seeded workspace; nothing was reset.',
        ],
      }
      await writeFile(
        join(CAPTURES_DIR, `steps-run-${runIndex}.json`),
        `${JSON.stringify(report, null, 2)}\n`,
      )
      await writeFile(
        join(CAPTURES_DIR, `steps-run-${runIndex}.md`),
        renderMarkdown(report, workspace.base, workspace.dbPath, traceName),
      )

      workspace.state.runs.push({
        index: runIndex,
        startedAtMs,
        finishedAtMs,
        commit: workspaceCommit(),
        steps: totals.steps,
        failures: totals.failures,
      })
      await workspace.save()

      // The session is evidence, not a pass/fail gate: it only fails when it
      // could not record anything at all.
      testInfo.attach('steps', {
        body: JSON.stringify(report, null, 2),
        contentType: 'application/json',
      })
      if (steps.length === 0) throw new Error('the session recorded no steps')
    } finally {
      await context.tracing.stop().catch(() => undefined)
      await workspace.stop()
    }
  })
})

function renderMarkdown(
  report: SessionReport,
  base: string,
  dbPath: string,
  traceName: string,
): string {
  const lines: string[] = [
    `# Dogfooding run ${report.runIndex} — automated interactive dogfooding`,
    '',
    `- started: ${new Date(report.startedAtMs).toISOString()}`,
    `- finished: ${new Date(report.finishedAtMs).toISOString()}`,
    `- server: ${base} (database \`${dbPath}\`)`,
    `- resumed an existing workspace: ${report.resumed ? 'yes' : 'no'}`,
    `- trace: \`captures/${traceName}\``,
    '',
    '## Steps',
    '',
    '| id | action | expected | observed | status | ms | capture |',
    '| --- | --- | --- | --- | --- | --- | --- |',
  ]
  for (const step of report.steps) {
    lines.push(
      `| ${step.id} | ${escapeCell(step.action)} | ${escapeCell(step.expected)} | ${escapeCell(step.observed)} | ${step.status} | ${step.durationMs} | ${step.screenshot ?? '—'} |`,
    )
  }
  lines.push('', '## Friction', '')
  const friction = report.steps.flatMap((step) =>
    step.friction.map((entry) => `- \`${step.id}\`: ${entry}`),
  )
  lines.push(...(friction.length > 0 ? friction : ['- none recorded']))
  lines.push('', '## Slow requests (≥ 1s)', '')
  lines.push(
    ...(report.slowRequests.length > 0
      ? report.slowRequests.map(
          (entry) => `- ${entry.ms} ms — ${entry.method} ${entry.url} → ${entry.status}`,
        )
      : ['- none']),
  )
  lines.push('', '## Browser errors', '')
  lines.push(...(report.browserErrors.length > 0 ? report.browserErrors.map((e) => `- ${e}`) : ['- none']))
  lines.push('', '## Refused requests (≥ 400)', '')
  lines.push(
    ...(report.failedResponses.length > 0
      ? report.failedResponses.map((entry) => `- ${entry.status} — ${entry.method} ${entry.url}`)
      : ['- none']),
  )
  return `${lines.join('\n')}\n`
}

function escapeCell(value: string): string {
  return value.replace(/\|/g, '\\|').replace(/\n/g, ' ')
}
