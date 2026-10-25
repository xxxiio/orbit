/**
 * I3 acceptance: work items are created, edited and transitioned through the
 * manifest's own declarations.
 *
 * Every assertion about a change is read back from the same server; the sheet
 * never moves an item locally. The guarded workflow (`p-guarded`) is published
 * by the deterministic fixture so a declared move can be refused by the server
 * for a reason the panel already names.
 */
import { expect, test } from './harness/fixtures'
import type { Locator, Page } from '@playwright/test'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

interface ApiWorkItem {
  id: string
  project_id: string
  kind: string
  state: string
  properties: Record<string, unknown>
  revision: number
}

async function workItems(page: Page, server: E2EServer, projectId: string): Promise<ApiWorkItem[]> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/blueprints/items?project_id=${projectId}`,
  )
  expect(response.status()).toBe(200)
  return (await response.json()) as ApiWorkItem[]
}

async function createdItem(
  page: Page,
  server: E2EServer,
  projectId: string,
  before: Set<string>,
): Promise<ApiWorkItem> {
  await expect
    .poll(async () => (await workItems(page, server, projectId)).length, { timeout: 10_000 })
    .toBeGreaterThan(before.size)
  const item = (await workItems(page, server, projectId)).find((candidate) => !before.has(candidate.id))
  expect(item, 'the server must store the new work item').toBeTruthy()
  return item!
}

function sheet(page: Page): Locator {
  return page.getByRole('dialog')
}

test.describe('work items', () => {
  test('create, edit a typed property, move through a declared transition', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-orbit-development'
    await goto(page, server, `/projects/${projectId}/board`)
    await expect(page.getByRole('group', { name: 'Work item board' })).toBeVisible()

    const before = new Set((await workItems(page, server, projectId)).map((item) => item.id))

    await page.getByRole('button', { name: 'New work item' }).first().click()
    const create = page.getByRole('dialog')
    await expect(create.getByRole('heading', { name: 'New work item' })).toBeVisible()
    // The view does not pin a kind and Development declares exactly one, so the
    // dialog names it without a pointless selector.
    await expect(create.getByText('Feature', { exact: true })).toBeVisible()
    await create.getByLabel('Priority').click()
    await page.getByRole('option', { name: 'High', exact: true }).click()
    await create.getByRole('button', { name: 'Create work item' }).click()
    await expect(create).toBeHidden()

    const item = await createdItem(page, server, projectId, before)
    expect(item.kind).toBe('feature')
    expect(item.state).toBe('backlog')
    expect(item.properties.priority).toBe('high')

    const card = page.locator(`[data-work-item-key="${item.id}"]`)
    await expect(card).toBeVisible()
    await expect(
      page.getByRole('region', { name: /^Backlog column/ }).locator(`[data-work-item-key="${item.id}"]`),
    ).toBeVisible()

    await card.getByRole('button').first().click()
    const panel = sheet(page)
    await expect(panel).toBeVisible()
    await expect(panel.getByText('Revision 0').first()).toBeVisible()

    // Typed property: the declared choice control writes one property change
    // with the revision read at mutate time.
    await panel.getByLabel('Priority').click()
    await page.getByRole('option', { name: 'Low', exact: true }).click()
    await expect(panel.getByText('Priority saved.').first()).toBeVisible()
    await expect(panel.getByText('Revision 1').first()).toBeVisible()
    expect((await workItems(page, server, projectId)).find((c) => c.id === item.id)?.properties.priority).toBe(
      'low',
    )

    // Declared moves only. `backlog → ready` is the manifest's first edge.
    await panel.getByRole('button', { name: 'Move work item to Ready' }).click()
    await expect(panel.getByText('The server moved this item to Ready.').first()).toBeVisible()
    await expect(panel.getByText('Revision 2').first()).toBeVisible()

    const moved = (await workItems(page, server, projectId)).find((c) => c.id === item.id)
    expect(moved?.state).toBe('ready')
    await expect(
      panel.getByRole('region', { name: 'Moves' }).getByText('Move to Implementing').first(),
    ).toBeVisible()

    // Capability execution is not offered here, and the panel says why.
    const actions = panel.getByRole('region', { name: 'Actions' })
    await expect(actions.getByText('Execution requires the local Orbit authority.')).toBeVisible()
    await expect(actions.getByText('Capability status is not exposed by this server.')).toBeVisible()
    await expect(actions.getByText(/This state declares no actions/)).toBeVisible()
    await expect(actions.getByRole('button')).toHaveCount(0)

    await page.reload()
    await expect(sheet(page)).toBeVisible()
    await page.getByRole('button', { name: 'Close work item details' }).click()
    await expect(sheet(page)).toBeHidden()
    await expect(page.locator(`[data-work-item-key="${item.id}"]`)).toBeVisible()
    await expect(
      page.getByRole('region', { name: /^Ready column/ }).locator(`[data-work-item-key="${item.id}"]`),
    ).toBeVisible()
  })

  test('a declared move refused by the server keeps the item in place and quotes the message', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-guarded'
    await goto(page, server, `/projects/${projectId}/overview`)
    const before = new Set((await workItems(page, server, projectId)).map((item) => item.id))

    await page.getByRole('button', { name: 'New work item' }).first().click()
    const create = page.getByRole('dialog')
    await create.getByRole('button', { name: 'Create work item' }).click()
    await expect(create).toBeHidden()
    const item = await createdItem(page, server, projectId, before)
    expect(item.properties.priority).toBe('low')

    await page.locator(`[data-work-item-key="${item.id}"]`).getByRole('button').first().click()
    const panel = sheet(page)
    await expect(panel).toBeVisible()
    // The guard is named before the attempt, so the refusal is not a surprise.
    await expect(
      panel.getByText(/Requires Priority to be High\./),
    ).toBeVisible()

    await panel.getByRole('button', { name: 'Move work item to Approved' }).click()
    const alert = panel.getByRole('alert').filter({ hasText: /server refused/i }).first()
    await expect(alert).toBeVisible()
    // The quoted sentence is the server's own error envelope.
    await expect(alert).toContainText('workflow operation rejected')
    await expect(panel.getByText('Revision 0').first()).toBeVisible()
    expect((await workItems(page, server, projectId)).find((c) => c.id === item.id)?.state).toBe(
      'open',
    )

    // Change the guarded property, then the same declared move is accepted.
    await panel.getByLabel('Priority').click()
    await page.getByRole('option', { name: 'High', exact: true }).click()
    await expect(panel.getByText('Priority saved.').first()).toBeVisible()
    await expect(panel.getByText('Revision 1').first()).toBeVisible()

    await panel.getByRole('button', { name: 'Move work item to Approved' }).click()
    await expect(panel.getByText('The server moved this item to Approved.').first()).toBeVisible()
    expect((await workItems(page, server, projectId)).find((c) => c.id === item.id)?.state).toBe(
      'approved',
    )

    await page.reload()
    await expect(sheet(page)).toBeVisible()
    await page.getByRole('button', { name: 'Close work item details' }).click()
    await expect(sheet(page)).toBeHidden()
    await expect(
      page.getByRole('region', { name: /^Approved column/ }).locator(`[data-work-item-key="${item.id}"]`),
    ).toBeVisible()
  })

})
