/**
 * I4 acceptance: connected capability execution in the browser.
 *
 * The server persists an authorized, fenced intent and never runs provider
 * code. A separately enrolled process (`orbit capability serve`) claims it,
 * runs the provider locally, reports one outcome, and the authority persists the
 * declared effect in the same savepoint as the state and the receipt.
 *
 * This file proves that whole cycle against a real server and the real CLI.
 * What the browser may not do is part of the proof: installation and grants are
 * operator routes (bearer only), and claiming/reporting are executor routes
 * (bearer only). No spec clicks a control for either — there is none — and each
 * refusal is asserted where the server actually draws the line.
 */
import { expect, test } from './harness/fixtures'
import type { Page } from '@playwright/test'
import { enrollDeviceWithAdminKey, type E2EServer } from './harness/server'
import { goto, signInViaApi } from './harness/session'
import {
  REFERENCE_ACTION,
  REFERENCE_CAPABILITY,
  announceCapability,
  authorizeCapability,
  buildCapabilityWorkspace,
  claimCapabilityAction,
  createWorkItem,
  postCapability,
  publishReferenceWorkflow,
  readCapabilityStatus,
  readEffects,
  readOffer,
  readRequests,
  readShow,
  readWorkItem,
  reportCapabilityResult,
  requestCapabilityAction,
  runExecutor,
  workItemNote,
  type CapabilityWorkspace,
} from './harness/capability'

/** The sheet is the drawer whose title ends in “work item details”. */
function sheet(page: Page) {
  return page.getByRole('dialog', { name: /work item details$/ })
}

function panel(page: Page) {
  return page.locator('[data-capability-panel]')
}

/** Navigate straight to the open item; the sheet is URL state (`?item=`). */
async function openItem(
  page: Page,
  server: E2EServer,
  workspace: CapabilityWorkspace,
): Promise<void> {
  await goto(page, server, `/projects/${workspace.projectId}/overview?item=${workspace.itemId}`)
  await expect(sheet(page)).toBeVisible()
  await expect(panel(page)).toBeVisible()
}

/** The one request the panel recorded, as the server read returns it. */
async function onlyRequest(page: Page, server: E2EServer, itemId: string) {
  const requests = await readRequests(page, server, itemId)
  expect(requests, `exactly one request must be stored for ${itemId}`).toHaveLength(1)
  return requests[0]
}

test.describe('connected capability execution', () => {
  test('a person requests the reference action, the executor runs it, and the note is durable', async ({
    page,
    server,
  }) => {
    const workspace = await buildCapabilityWorkspace(page, server)
    const { projectId, itemId } = workspace

    // ----------------------------------------------------------------
    // The panel reads durable facts, not intent.
    // ----------------------------------------------------------------
    await openItem(page, server, workspace)
    const capabilityRow = panel(page).locator(`[data-capability-status="${REFERENCE_CAPABILITY}"]`)
    await expect(capabilityRow).toHaveAttribute('data-capability-installed', 'true')
    await expect(capabilityRow).toHaveAttribute('data-capability-granted', 'true')
    await expect(capabilityRow).toHaveAttribute('data-capability-executors', '1')
    await expect(capabilityRow.getByText('Installed', { exact: true })).toBeVisible()
    await expect(capabilityRow.getByText('Granted for this project', { exact: true })).toBeVisible()
    await expect(capabilityRow.getByText('Executor announced', { exact: true })).toBeVisible()

    const offerRow = panel(page).locator(`[data-capability-offer="${REFERENCE_ACTION}"]`)
    await expect(offerRow).toHaveAttribute('data-capability-available', 'true')
    await expect(offerRow.getByText('Available to request', { exact: true })).toBeVisible()

    // The same availability, read from the server, before any of it is clicked.
    const offer = await readOffer(page, server, itemId)
    expect(offer.available).toBe(true)
    expect(offer.code).toBeNull()
    expect(offer.capability_id).toBe(REFERENCE_CAPABILITY)
    expect(offer.from).toBe('open')
    expect(offer.to).toBe('closed')
    expect(offer.input_schema.required).toEqual(['note'])
    // The same three durable facts the badges claim, read by this device.
    const status = await readCapabilityStatus(page, server, projectId)
    const reference = status.find((entry) => entry.capability_id === REFERENCE_CAPABILITY)
    expect(reference, 'the server catalog must list the reference capability').toBeTruthy()
    expect(reference?.installed).toBe(true)
    expect(reference?.granted).toBe(true)
    expect(reference?.executors.map((executor) => executor.device_id)).toEqual([server.deviceId])

    // ----------------------------------------------------------------
    // Request it through the UI, with the explicit confirmation step.
    // ----------------------------------------------------------------
    const note = 'Durable note from the connected executor'
    const tag = 'acceptance'

    await offerRow.getByRole('button', { name: 'Request this action' }).click()
    const form = panel(page).locator(`[data-capability-request-form="${REFERENCE_ACTION}"]`)
    await expect(form).toBeVisible()
    await form.locator('[data-capability-input="note"] textarea').fill(note)
    await form.locator('[data-capability-input="tag"] input').fill(tag)

    await form.getByRole('button', { name: 'Review the request' }).click()
    // The confirmation is a dialog: Radix portals it outside the sheet, so it is
    // located from the page, not from the panel.
    const confirm = page.locator(`[data-capability-confirm="${REFERENCE_ACTION}"]`)
    await expect(confirm).toBeVisible()
    // The confirmation states the effect and that an executor — not this
    // browser — runs it.
    await expect(page.locator('[data-capability-confirm-effect]')).toContainText(
      'changes this work item’s stored values',
    )
    await confirm.getByRole('button', { name: 'Record Append note' }).click()

    // ----------------------------------------------------------------
    // Pending: recorded, not run.
    // ----------------------------------------------------------------
    const requestRow = panel(page).locator('[data-capability-request]').first()
    await expect(requestRow).toHaveAttribute('data-capability-state', 'pending')
    await expect(
      requestRow.getByText('Waiting for an executor to claim it. Nothing has run.'),
    ).toBeVisible()
    const commandId = await requestRow.getAttribute('data-capability-request')
    expect(commandId, 'the panel must name the recorded command').toBeTruthy()

    const pending = await onlyRequest(page, server, itemId)
    expect(pending.command_id).toBe(commandId)
    expect(pending.state).toBe('pending')
    expect(pending.inputs).toEqual({ note, tag })
    expect(pending.executor_device_id).toBeNull()

    // Nothing has moved: the request is not a transition.
    const beforeRun = await readWorkItem(page, server, itemId)
    expect(beforeRun.state).toBe('open')
    expect(beforeRun.revision).toBe(0)

    // ----------------------------------------------------------------
    // The real executor claims, runs and reports.
    // ----------------------------------------------------------------
    const executor = await runExecutor(server)
    expect(
      executor.result.status,
      `the executor must report a run, not idle or a refusal: ${executor.stdout}`,
    ).toBe('reported')
    expect(executor.result.outcome).toBe('succeeded')
    expect(executor.result.state).toBe('succeeded')
    expect(executor.result.action.command_id).toBe(commandId)
    expect(executor.result.action.item_id).toBe(itemId)
    expect(executor.result.receipt.reported_by).toBe(server.deviceId)
    // The executor says where the provider wrote and that the declared effect
    // travelled with the report.
    expect(executor.result.local.effect.submitted_to_authority).toBe(true)
    expect(executor.result.local.submitted_effect).toEqual({
      kind: 'work_item_note',
      item_id: itemId,
      note,
      tag,
    })

    // ----------------------------------------------------------------
    // The browser reaches the settled state and shows the receipt.
    // ----------------------------------------------------------------
    await expect(requestRow).toHaveAttribute('data-capability-state', 'succeeded', {
      timeout: 30_000,
    })
    await expect(
      requestRow.getByText(
        'An executor reported success. The item’s stored values are read from the server; this report does not move anything by itself.',
      ),
    ).toBeVisible()
    const receipt = requestRow.locator(`[data-capability-receipt="${commandId}"]`)
    await expect(receipt).toBeVisible({ timeout: 20_000 })
    await expect(receipt).toContainText('Outcome')
    await expect(receipt).toContainText('succeeded')
    await expect(receipt).toContainText(server.deviceId)

    // ----------------------------------------------------------------
    // The stored result: one note, exactly the text that was typed.
    // ----------------------------------------------------------------
    const effects = await readEffects(page, server, itemId)
    expect(effects, 'one command id can never produce two notes').toHaveLength(1)
    expect(effects[0].command_id).toBe(commandId)
    expect(effects[0].note).toBe(note)
    expect(effects[0].tag).toBe(tag)

    const settled = await readShow(page, server, commandId as string)
    expect(settled.request.state).toBe('succeeded')
    expect(settled.receipt?.outcome).toBe('succeeded')
    expect(settled.receipt?.reported_by).toBe(server.deviceId)

    // The receipt never moves the item: same state, same revision, on screen
    // and on the server.
    const afterRun = await readWorkItem(page, server, itemId)
    expect(afterRun.state).toBe('open')
    expect(afterRun.revision).toBe(0)
    await expect(sheet(page).locator('header').getByText('Revision 0')).toBeVisible()
    await expect(sheet(page).locator('header').getByText('Open', { exact: true })).toBeVisible()
    await expect(
      sheet(page).getByRole('region', { name: 'Declared values' }).getByText('Open', { exact: true }),
    ).toBeVisible()
  })

  test('availability names one refusal at a time and no unavailable action has a request control', async ({
    page,
    server,
  }) => {
    await signInViaApi(page, server)
    // A publish before the install is refused: the capability is not installed,
    // and the server validates a manifest against its durable installation set.
    const early = await publishReferenceWorkflow(page, server)
    expect(early.status, JSON.stringify(early.body)).toBe(400)
    expect(String(early.body?.error ?? '')).toContain('blueprint operation rejected')

    const workspace = await buildCapabilityWorkspace(page, server, {
      authorize: false,
      announce: false,
    })
    const { projectId, itemId } = workspace

    // No executor has announced it: the offer says exactly that, and says
    // nothing about authorization.
    const noExecutor = await readOffer(page, server, itemId)
    expect(noExecutor.available).toBe(false)
    expect(noExecutor.code).toBe('executor_missing')
    expect(noExecutor.reason).toContain('no active enrolled executor announced')

    await openItem(page, server, workspace)
    const offerRow = panel(page).locator(`[data-capability-offer="${REFERENCE_ACTION}"]`)
    await expect(offerRow).toHaveAttribute('data-capability-available', 'false')
    await expect(offerRow).toHaveAttribute('data-capability-code', 'executor_missing')
    const unavailable = offerRow.locator('[data-capability-unavailable="executor_missing"]')
    await expect(unavailable).toBeVisible()
    await expect(unavailable).toContainText('No executor has announced this capability.')
    // An unavailable action is described, never offered as a control.
    await expect(panel(page).locator('[data-capability-request-open]')).toHaveCount(0)

    // Announce, but do not grant: now the durable refusal is the grant.
    const announced = await announceCapability(page, server)
    expect(announced.status, JSON.stringify(announced.body)).toBe(200)
    await openItem(page, server, workspace)
    const unauthorized = await readOffer(page, server, itemId)
    expect(unauthorized.available).toBe(false)
    expect(unauthorized.code).toBe('not_authorized')
    expect(unauthorized.code).not.toBe('executor_missing')
    expect(unauthorized.reason).toContain(`is not authorized for project ${projectId}`)
    await expect(offerRow).toHaveAttribute('data-capability-code', 'not_authorized')
    const notGranted = offerRow.locator('[data-capability-unavailable="not_authorized"]')
    await expect(notGranted).toBeVisible()
    await expect(notGranted).toContainText('This project has not been granted this capability.')
    await expect(panel(page).locator('[data-capability-request-open]')).toHaveCount(0)

    // The browser is not an operator: install and authorize are bearer-only.
    // Asserted where the server refuses, not by hunting for a missing button.
    const cookieInstall = await postCapability(page, server, 'capabilities/install', {
      command_id: `install-cookie-${Date.now()}`,
      capability_id: REFERENCE_CAPABILITY,
    })
    expect(cookieInstall.status).toBe(403)
    expect(String(cookieInstall.body?.error ?? '')).toContain('operator credential required')
    const cookieAuthorize = await postCapability(page, server, 'capabilities/authorize', {
      command_id: `authorize-cookie-${Date.now()}`,
      capability_id: REFERENCE_CAPABILITY,
      project_id: projectId,
      confirm: true,
    })
    expect(cookieAuthorize.status).toBe(403)
    expect(String(cookieAuthorize.body?.error ?? '')).toContain('operator credential required')
    // …and the browser is not an executor either.
    const cookieAnnounce = await postCapability(page, server, 'capability-executors/announce', {
      capability_ids: [REFERENCE_CAPABILITY],
    })
    expect(cookieAnnounce.status).toBe(403)
    expect(String(cookieAnnounce.body?.error ?? '')).toContain('executor credential required')

    // Grant it as the operator (bearer), then the offer is available.
    const grant = await authorizeCapability(page, server, projectId)
    expect(grant.status, JSON.stringify(grant.body)).toBe(200)
    await openItem(page, server, workspace)
    const available = await readOffer(page, server, itemId)
    expect(available.available).toBe(true)
    expect(available.code).toBeNull()
    await expect(offerRow).toHaveAttribute('data-capability-available', 'true')
    await expect(panel(page).locator('[data-capability-request-open]')).toHaveCount(1)

    // An empty note is refused before anything is sent.
    const posted: string[] = []
    const record = (request: { method(): string; url(): string }) => {
      if (
        request.method() === 'POST' &&
        request.url().includes('/v1/authoritative/capability-actions/request')
      ) {
        posted.push(request.url())
      }
    }
    page.on('request', record)
    try {
      await panel(page).locator(`[data-capability-offer="${REFERENCE_ACTION}"]`)
        .getByRole('button', { name: 'Request this action' })
        .click()
      const form = panel(page).locator(`[data-capability-request-form="${REFERENCE_ACTION}"]`)
      await expect(form).toBeVisible()
      await form.getByRole('button', { name: 'Review the request' }).click()
      await expect(form.getByText('Note is required.')).toBeVisible()
      // The confirmation step was never reached, and nothing was posted.
      await expect(page.locator(`[data-capability-confirm="${REFERENCE_ACTION}"]`)).toHaveCount(0)
      expect(posted, 'an empty required note must not reach the server').toEqual([])
    } finally {
      page.off('request', record)
    }

    // A stale revision is the server's 409, with its own fixed sentence.
    const stale = await requestCapabilityAction(page, server, {
      commandId: `stale-${Date.now()}`,
      itemId,
      expectedRevision: 7,
      inputs: { note: 'stale' },
    })
    expect(stale.status, JSON.stringify(stale.body)).toBe(409)
    expect(String(stale.body?.error ?? '')).toContain('capability request conflict')
    expect(await readRequests(page, server, itemId)).toHaveLength(0)

    // A body cannot invent the target state or the provider.
    const forged = await postCapability(page, server, 'capability-actions/request', {
      command_id: `forged-${Date.now()}`,
      item_id: itemId,
      expected_revision: 0,
      action_id: REFERENCE_ACTION,
      to: 'closed',
      provider: 'evil',
      inputs: { note: 'forged' },
    })
    expect(forged.status, JSON.stringify(forged.body)).toBe(422)
    expect(await readRequests(page, server, itemId)).toHaveLength(0)
  })

  test('one claim, one receipt and one note: a second executor cannot take it and a replay is a replay', async ({
    page,
    server,
  }) => {
    const workspace = await buildCapabilityWorkspace(page, server)
    const { itemId } = workspace

    // A second enrolled device that also announces the capability.
    const second = {
      deviceId: `e2e-${Date.now().toString(36)}-executor-two`,
      token: `second-executor-token-${'x'.repeat(32)}`,
    }
    await enrollDeviceWithAdminKey(server, second)
    const secondAnnounce = await announceCapability(page, server, second)
    expect(secondAnnounce.status, JSON.stringify(secondAnnounce.body)).toBe(200)

    const commandId = `single-claim-${Date.now()}`
    const recorded = await requestCapabilityAction(page, server, {
      commandId,
      itemId,
      expectedRevision: 0,
      inputs: { note: 'claimed once', tag: 'single' },
    })
    expect(recorded.status, JSON.stringify(recorded.body)).toBe(200)
    expect(recorded.body?.request.state).toBe('pending')

    // First executor claims it with its own token.
    const firstClaim = await claimCapabilityAction(page, server, {
      commandId,
      claimToken: 'claim-first',
    })
    expect(firstClaim.status, JSON.stringify(firstClaim.body)).toBe(200)
    expect(firstClaim.body?.request.state).toBe('claimed')
    expect(firstClaim.body?.request.executor_device_id).toBe(server.deviceId)
    expect(firstClaim.body?.execution_plan.parameters).toEqual({
      note: 'claimed once',
      tag: 'single',
    })

    // A different device's bearer credential is refused with the fixed sentence.
    const secondClaim = await claimCapabilityAction(
      page,
      server,
      { commandId, claimToken: 'claim-second' },
      second,
    )
    expect(secondClaim.status, JSON.stringify(secondClaim.body)).toBe(409)
    expect(String(secondClaim.body?.error ?? '')).toContain('capability action already claimed')
    // …and it cannot report a result for a claim it never held either.
    const secondReport = await reportCapabilityResult(
      page,
      server,
      {
        commandId,
        claimToken: 'claim-second',
        outcome: 'succeeded',
        effect: workItemNote(itemId, 'from the losing device'),
      },
      second,
    )
    expect(secondReport.status, JSON.stringify(secondReport.body)).toBe(409)
    expect(String(secondReport.body?.error ?? '')).toContain('capability action already claimed')

    // The claiming executor reports once, with its declared effect.
    const reported = await reportCapabilityResult(page, server, {
      commandId,
      claimToken: 'claim-first',
      outcome: 'succeeded',
      detail: 'the provider wrote the note',
      effect: workItemNote(itemId, 'claimed once', 'single'),
    })
    expect(reported.status, JSON.stringify(reported.body)).toBe(200)
    const receipt = reported.body?.receipt
    expect(reported.body?.request.state).toBe('succeeded')
    expect(receipt.outcome).toBe('succeeded')
    expect(receipt.reported_by).toBe(server.deviceId)
    expect(receipt.detail.effect).toEqual({
      applied: true,
      kind: 'work_item_note',
      item_id: itemId,
      command_id: commandId,
    })

    // Replaying the same report returns the original receipt, not a second one.
    const replay = await reportCapabilityResult(page, server, {
      commandId,
      claimToken: 'claim-first',
      outcome: 'succeeded',
      detail: 'the provider wrote the note',
      effect: workItemNote(itemId, 'claimed once', 'single'),
    })
    expect(replay.status, JSON.stringify(replay.body)).toBe(200)
    expect(replay.body?.receipt).toEqual(receipt)
    expect(replay.body?.request.state).toBe('succeeded')

    const effects = await readEffects(page, server, itemId)
    expect(effects, 'one command id can never produce two notes').toHaveLength(1)
    expect(effects[0]).toMatchObject({ command_id: commandId, note: 'claimed once', tag: 'single' })

    // A real executor run has nothing left to claim: idle, still one note.
    const idle = await runExecutor(server)
    expect(idle.result.status, idle.stdout).toBe('idle')
    expect(idle.result.command_id).toBeUndefined()
    expect(await readEffects(page, server, itemId)).toHaveLength(1)

    // The browser agrees with all of it: settled, one receipt, one request.
    await openItem(page, server, workspace)
    const requestRow = panel(page).locator(`[data-capability-request="${commandId}"]`)
    await expect(requestRow).toHaveAttribute('data-capability-state', 'succeeded')
    await expect(requestRow.locator(`[data-capability-receipt="${commandId}"]`)).toContainText(
      'succeeded',
    )
    await expect(panel(page).locator('[data-capability-request]')).toHaveCount(1)
  })

  test('an uncertain outcome is settled by an explicit operator decision, never by a retry', async ({
    page,
    server,
  }) => {
    // The executor is a *different* enrolled device, so the receipt can prove
    // who ran it and who later decided. Announce only that device: the browser
    // is an operator here, never an executor.
    const executor = {
      deviceId: `e2e-${Date.now().toString(36)}-uncertain-executor`,
      token: `uncertain-executor-token-${'y'.repeat(32)}`,
    }
    await enrollDeviceWithAdminKey(server, executor)
    const executorAnnounce = await announceCapability(page, server, executor)
    expect(executorAnnounce.status, JSON.stringify(executorAnnounce.body)).toBe(200)

    const workspace = await buildCapabilityWorkspace(page, server, { announce: false })
    const { itemId } = workspace

    const commandId = `uncertain-${Date.now()}`
    const recorded = await requestCapabilityAction(page, server, {
      commandId,
      itemId,
      expectedRevision: 0,
      inputs: { note: 'the provider crashed' },
    })
    expect(recorded.status, JSON.stringify(recorded.body)).toBe(200)

    // The claiming executor reports that it could not tell. This is the
    // executor route, so it is driven with that device's bearer credential
    // directly: the panel has no control that can report an outcome.
    const claimed = await claimCapabilityAction(
      page,
      server,
      { commandId, claimToken: 'claim-uncertain' },
      executor,
    )
    expect(claimed.status, JSON.stringify(claimed.body)).toBe(200)
    const reported = await reportCapabilityResult(
      page,
      server,
      {
        commandId,
        claimToken: 'claim-uncertain',
        outcome: 'uncertain',
        detail: 'the provider exited before it could say what it wrote',
      },
      executor,
    )
    expect(reported.status, JSON.stringify(reported.body)).toBe(200)
    expect(reported.body?.request.state).toBe('uncertain')
    expect(reported.body?.receipt.outcome).toBe('uncertain')
    expect(reported.body?.receipt.reported_by).toBe(executor.deviceId)

    await openItem(page, server, workspace)
    const requestRow = panel(page).locator(`[data-capability-request="${commandId}"]`)
    await expect(requestRow).toHaveAttribute('data-capability-state', 'uncertain')
    await expect(requestRow.getByText('Uncertain', { exact: true })).toBeVisible()
    // Nothing here re-runs the action: no retry, no re-send, no second claim.
    await expect(requestRow.getByRole('button', { name: /retry|re-?run|run again|re-?send/i })).toHaveCount(
      0,
    )
    // The only settlement control is the explicit decision.
    const reconcile = requestRow.locator(`[data-capability-reconcile="${commandId}"]`)
    await expect(reconcile).toBeVisible()
    await reconcile.getByRole('radio', { name: /Confirm it succeeded/ }).check()
    await reconcile.getByRole('button', { name: 'Record this decision' }).click()

    // The decision settles the row: the panel re-reads the intent, which now
    // reads `succeeded`, and the reconcile form is gone because a settled
    // request can never be reconciled again.
    await expect(requestRow).toHaveAttribute('data-capability-state', 'succeeded')
    await expect(requestRow.getByText('Succeeded', { exact: true })).toBeVisible()
    await expect(requestRow.locator(`[data-capability-reconcile="${commandId}"]`)).toHaveCount(0)

    // The operator device — this browser's device, not the executor — is the
    // recorded actor, and the recovery is named.
    const shown = await readShow(page, server, commandId)
    expect(shown.request.state).toBe('succeeded')
    expect(shown.receipt?.outcome).toBe('succeeded')
    expect(shown.receipt?.reported_by).toBe(server.deviceId)
    expect(shown.receipt?.detail).toMatchObject({
      reconciled: true,
      reconciled_by: server.deviceId,
      recovered_from: 'uncertain',
    })
    const receipt = requestRow.locator(`[data-capability-receipt="${commandId}"]`)
    await expect(receipt).toBeVisible({ timeout: 20_000 })
    await expect(receipt).toContainText(`"reconciled_by": "${server.deviceId}"`)
    await expect(receipt).toContainText(`"recovered_from": "uncertain"`)
    await expect(receipt).toContainText(server.deviceId)
    expect(server.deviceId).not.toBe(executor.deviceId)
    // A decision settles the question; it never invents an effect the provider
    // did not produce.
    expect(await readEffects(page, server, itemId)).toHaveLength(0)

    // A pending request cannot be reconciled, so the panel offers no decision.
    const pendingItem = workspace.secondItemId
    const created = await createWorkItem(page, server, { itemId: pendingItem, projectId: workspace.projectId })
    expect(created.status, JSON.stringify(created.body)).toBe(200)
    const pendingCommand = `pending-${Date.now()}`
    const pending = await requestCapabilityAction(page, server, {
      commandId: pendingCommand,
      itemId: pendingItem,
      expectedRevision: 0,
      inputs: { note: 'never claimed' },
    })
    expect(pending.status, JSON.stringify(pending.body)).toBe(200)

    await goto(page, server, `/projects/${workspace.projectId}/overview?item=${pendingItem}`)
    await expect(sheet(page)).toBeVisible()
    const pendingRow = page.locator(`[data-capability-request="${pendingCommand}"]`)
    await expect(pendingRow).toHaveAttribute('data-capability-state', 'pending')
    await expect(pendingRow.getByText('Pending', { exact: true })).toBeVisible()
    await expect(pendingRow.locator(`[data-capability-reconcile="${pendingCommand}"]`)).toHaveCount(0)
    await expect(page.locator(`[data-capability-reconciled="${pendingCommand}"]`)).toHaveCount(0)

    // ----------------------------------------------------------------
    // A `claimed` request with no receipt: an executor took it and nothing
    // recorded a result. The server settles this exactly like an `uncertain`
    // outcome (lost acknowledgement), so the panel must offer the same two
    // decisions — with copy that says which of the two questions this is.
    // ----------------------------------------------------------------
    const claimedItem = `${workspace.secondItemId}-claimed`
    const claimedCreated = await createWorkItem(page, server, {
      itemId: claimedItem,
      projectId: workspace.projectId,
    })
    expect(claimedCreated.status, JSON.stringify(claimedCreated.body)).toBe(200)

    // D1: the offer now carries the effect's own bound for `tag`, so a tag the
    // authority would refuse can no longer be requested in the first place.
    // Before that bound was declared, such a request was accepted, the executor
    // ran, the report was refused, and the request was stranded `claimed`.
    const boundedOffer = await readOffer(page, server, claimedItem)
    expect(boundedOffer.input_schema.properties.tag.maxLength).toBe(64)
    const overBound = await requestCapabilityAction(page, server, {
      commandId: `over-bound-${Date.now()}`,
      itemId: claimedItem,
      expectedRevision: 0,
      inputs: { note: 'a tag the effect contract would refuse', tag: 't'.repeat(65) },
    })
    expect(overBound.status, JSON.stringify(overBound.body)).toBe(400)
    expect(String(overBound.body?.error ?? '')).toContain('invalid capability action input')
    expect(
      await readRequests(page, server, claimedItem),
      'an input beyond the declared bound must never become a request',
    ).toHaveLength(0)

    const claimedCommand = `claimed-${Date.now()}`
    const claimedRecorded = await requestCapabilityAction(page, server, {
      commandId: claimedCommand,
      itemId: claimedItem,
      expectedRevision: 0,
      inputs: { note: 'claimed but never reported', tag: 'lost-ack' },
    })
    expect(claimedRecorded.status, JSON.stringify(claimedRecorded.body)).toBe(200)

    // The executor route claims the intent and then reports nothing at all: this
    // is the state a crashed executor leaves, and it needs no fabrication — a
    // claim without a result is exactly what the two routes allow.
    const claimedOnly = await claimCapabilityAction(
      page,
      server,
      { commandId: claimedCommand, claimToken: 'claim-lost-ack' },
      executor,
    )
    expect(claimedOnly.status, JSON.stringify(claimedOnly.body)).toBe(200)
    expect(claimedOnly.body?.request.state).toBe('claimed')
    expect(claimedOnly.body?.request.executor_device_id).toBe(executor.deviceId)
    const stuck = await readShow(page, server, claimedCommand)
    expect(stuck.request.state).toBe('claimed')
    expect(stuck.receipt).toBeNull()

    await goto(page, server, `/projects/${workspace.projectId}/overview?item=${claimedItem}`)
    await expect(sheet(page)).toBeVisible()
    const claimedRow = page.locator(`[data-capability-request="${claimedCommand}"]`)
    await expect(claimedRow).toHaveAttribute('data-capability-state', 'claimed')
    await expect(claimedRow.getByText('Claimed', { exact: true })).toBeVisible()
    await expect(claimedRow).toContainText(executor.deviceId)
    // Still no retry: a settlement records a decision, it never re-runs.
    await expect(
      claimedRow.getByRole('button', { name: /retry|re-?run|run again|re-?send/i }),
    ).toHaveCount(0)
    // The receipt read is not offered while nothing is recorded.
    await expect(claimedRow.locator(`[data-capability-receipt="${claimedCommand}"]`)).toHaveCount(0)

    const claimedReconcile = claimedRow.locator(`[data-capability-reconcile="${claimedCommand}"]`)
    await expect(claimedReconcile).toBeVisible()
    await expect(claimedReconcile).toHaveAttribute('data-capability-reconcile-state', 'claimed')
    // The copy is honest about the difference: this is silence after a claim, not
    // an executor admitting it could not tell.
    await expect(claimedReconcile).toContainText(
      'An executor took this request, and no result was recorded.',
    )
    await expect(claimedReconcile).toContainText('answer may have been lost')
    await expect(claimedReconcile).not.toContainText('could not tell whether its effect landed')
    // Both reconcilable states offer the same two decisions, and only those.
    await expect(claimedReconcile.getByRole('radio')).toHaveCount(2)
    await expect(claimedReconcile.getByRole('radio').nth(0)).toHaveAccessibleName(
      /Confirm it succeeded/,
    )
    await expect(claimedReconcile.getByRole('radio').nth(1)).toHaveAccessibleName(
      /Confirm it failed/,
    )
    await claimedReconcile.getByRole('radio', { name: /Confirm it succeeded/ }).check()
    await claimedReconcile.getByRole('button', { name: 'Record this decision' }).click()

    // Settled: the decision is durable, the control is gone, and the receipt
    // names the recovery and the reporter.
    await expect(claimedRow).toHaveAttribute('data-capability-state', 'succeeded')
    await expect(
      claimedRow.locator(`[data-capability-reconcile="${claimedCommand}"]`),
    ).toHaveCount(0)
    const reconciledClaim = await readShow(page, server, claimedCommand)
    expect(reconciledClaim.request.state).toBe('succeeded')
    expect(reconciledClaim.receipt?.outcome).toBe('succeeded')
    // The reporter is this browser's device — the operator who decided — not the
    // executor that claimed the request.
    expect(reconciledClaim.receipt?.reported_by).toBe(server.deviceId)
    expect(reconciledClaim.receipt?.detail).toMatchObject({
      reconciled: true,
      reconciled_by: server.deviceId,
      recovered_from: 'claimed',
    })
    const claimedReceipt = claimedRow.locator(`[data-capability-receipt="${claimedCommand}"]`)
    await expect(claimedReceipt).toBeVisible({ timeout: 20_000 })
    await expect(claimedReceipt).toContainText('"reconciled": true')
    await expect(claimedReceipt).toContainText(`"recovered_from": "claimed"`)
    await expect(claimedReceipt).toContainText(`"reconciled_by": "${server.deviceId}"`)
    await expect(claimedReceipt).toContainText(server.deviceId)
    expect(server.deviceId).not.toBe(executor.deviceId)
    // A decision settles the question; it never invents an effect.
    expect(await readEffects(page, server, claimedItem)).toHaveLength(0)
  })

  test('a restarted server keeps the claim, the receipt and the note; a second executor is idle', async ({
    page,
    server,
  }) => {
    const workspace = await buildCapabilityWorkspace(page, server)
    const { itemId } = workspace

    const commandId = `restart-${Date.now()}`
    const note = 'this note survives a restart'
    const recorded = await requestCapabilityAction(page, server, {
      commandId,
      itemId,
      expectedRevision: 0,
      inputs: { note, tag: 'restart' },
    })
    expect(recorded.status, JSON.stringify(recorded.body)).toBe(200)

    // The real executor settles it before the restart.
    const run = await runExecutor(server)
    expect(run.result.status, run.stdout).toBe('reported')
    expect(run.result.outcome).toBe('succeeded')
    expect(run.result.action.command_id).toBe(commandId)

    const beforeEffects = await readEffects(page, server, itemId)
    expect(beforeEffects).toHaveLength(1)
    const before = await readShow(page, server, commandId)
    expect(before.receipt?.outcome).toBe('succeeded')

    // A real process restart on the same database. Web sessions live in memory,
    // so the browser signs in again — the durable capability state must not care.
    const restarted = await server.restart()
    await signInViaApi(page, restarted)

    const after = await readShow(page, restarted, commandId)
    expect(after.request.state).toBe('succeeded')
    expect(after.request.executor_device_id).toBe(restarted.deviceId)
    expect(after.receipt).toEqual(before.receipt)
    const afterEffects = await readEffects(page, restarted, itemId)
    expect(afterEffects).toHaveLength(1)
    expect(afterEffects[0]).toMatchObject({ command_id: commandId, note, tag: 'restart' })
    const item = await readWorkItem(page, restarted, itemId)
    expect(item.state).toBe('open')
    expect(item.revision).toBe(0)

    // The visible result after the restart: settled request, receipt, unchanged item.
    await goto(
      page,
      restarted,
      `/projects/${workspace.projectId}/overview?item=${itemId}`,
    )
    await expect(sheet(page)).toBeVisible()
    const requestRow = page.locator(`[data-capability-request="${commandId}"]`)
    await expect(requestRow).toHaveAttribute('data-capability-state', 'succeeded')
    const receipt = requestRow.locator(`[data-capability-receipt="${commandId}"]`)
    await expect(receipt).toBeVisible({ timeout: 20_000 })
    await expect(receipt).toContainText('succeeded')
    await expect(sheet(page).locator('header').getByText('Revision 0')).toBeVisible()
    await expect(sheet(page).locator('header').getByText('Open', { exact: true })).toBeVisible()

    // Nothing is pending, so the executor has nothing to claim: idle, one note.
    const idle = await runExecutor(restarted)
    expect(idle.result.status, idle.stdout).toBe('idle')
    expect(await readEffects(page, restarted, itemId)).toHaveLength(1)
  })
})
