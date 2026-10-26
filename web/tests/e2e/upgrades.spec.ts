/**
 * I3 acceptance: workflow version upgrades are previewed from the server's
 * report, applied only with the previewed fingerprint and an explicit
 * confirmation, and leave an honest receipt.
 *
 * Each test publishes a derived Development v3 through the public API (never a
 * direct database write) so the isolated workspace has a real target version.
 */
import { expect, test } from './harness/fixtures'
import type { E2EServer } from './harness/server'
import { goto } from './harness/session'

interface Manifest {
  id: string
  version: number
  name: string
  required_capabilities: string[]
  work_items: Array<{
    id: string
    name: string
    initial_state: string
    states: string[]
    fields: Array<{ id: string; kind: Record<string, unknown>; required: boolean; default: unknown }>
    transitions: Array<Record<string, unknown>>
  }>
  views: Array<{
    id: string
    title: string
    description: string
    layout: { type: string }
    components: Array<Record<string, unknown>>
  }>
}

function bearer(server: E2EServer) {
  return {
    authorization: `Bearer ${server.token}`,
    'x-orbit-device-id': server.deviceId,
    'content-type': 'application/json',
  }
}

async function readManifest(
  page: import('@playwright/test').Page,
  server: E2EServer,
  id: string,
  version: number,
): Promise<Manifest> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/blueprints/show?id=${id}&version=${version}`,
    { headers: bearer(server) },
  )
  expect(response.status()).toBe(200)
  return (await response.json()) as Manifest
}

async function publish(
  page: import('@playwright/test').Page,
  server: E2EServer,
  definition: Manifest,
): Promise<void> {
  const response = await page.request.post(`${server.base}/v1/authoritative/blueprints`, {
    headers: bearer(server),
    data: { definition },
  })
  expect(response.status(), await response.text()).toBe(200)
}

async function bindingVersion(
  page: import('@playwright/test').Page,
  server: E2EServer,
  projectId: string,
): Promise<number> {
  const response = await page.request.get(
    `${server.base}/v1/authoritative/blueprints/binding?project_id=${projectId}`,
    { headers: bearer(server) },
  )
  expect(response.status()).toBe(200)
  return ((await response.json()) as { version: number }).version
}

test.describe('workflow upgrades', () => {
  test('an applicable upgrade previews, confirms and reflects the new views', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-orbit-development'
    const base = await readManifest(page, server, 'development', 2)
    const target: Manifest = structuredClone(base)
    target.version = 3
    target.work_items[0].fields.push({
      id: 'notes',
      kind: { type: 'text' },
      required: false,
      default: null,
    })
    target.views.push({
      id: 'release',
      title: 'Release notes',
      description: 'Work items planned for the next release.',
      layout: { type: 'vertical' },
      components: [
        {
          id: 'release_board',
          type: 'kanban',
          source: 'work_items',
          group_by: 'state',
          limit: 100,
          title: 'Release board',
        },
      ],
    })
    await publish(page, server, target)

    await goto(page, server, `/projects/${projectId}/settings`)

    // The default target is the newest published version of the same id.
    await expect(page.getByText(/A newer Development version is published/)).toBeVisible({ timeout: 20_000 })
    await expect(page.getByText('Development v2', { exact: true }).first()).toBeVisible()
    await expect(page.getByText(/^fingerprint /)).toBeVisible()
    await expect(page.getByText('Release notes').first()).toBeVisible()
    await expect(page.getByText('notes').first()).toBeVisible()

    // Nothing is applied by opening the preview.
    expect(await bindingVersion(page, server, projectId)).toBe(2)

    await page.getByRole('button', { name: 'Upgrade workflow…' }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()
    await expect(dialog.getByText(/^fingerprint /)).toBeVisible()

    // The confirmation is explicit and names the reviewed version.
    const confirm = dialog.getByRole('checkbox')
    await expect(confirm).not.toBeChecked()
    await expect(dialog.getByRole('button', { name: 'Apply upgrade' })).toBeDisabled()
    await confirm.check()
    await dialog.getByRole('button', { name: 'Apply upgrade' }).click()

    const receipt = page.locator('[data-upgrade-receipt]')
    await expect(receipt).toBeVisible({ timeout: 20_000 })
    await expect(receipt).toContainText('Workflow change applied')
    await expect(receipt).toContainText('v2 → Development v3')
    await expect(receipt).toContainText('fingerprint')
    expect(await bindingVersion(page, server, projectId)).toBe(3)

    // The new view is a tab immediately, and it survives a reload.
    const releaseTab = page.getByRole('link', { name: /^Release notes/ })
    await expect(releaseTab).toBeVisible({ timeout: 20_000 })
    await releaseTab.click()
    await expect(releaseTab).toHaveAttribute('aria-current', 'page', { timeout: 20_000 })
    await expect(page.getByRole('heading', { name: 'Release board' })).toBeVisible({ timeout: 20_000 })

    await page.reload()
    await expect(page.getByRole('link', { name: /^Release notes/ })).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Release board' })).toBeVisible()
  })

  test('a changed fingerprint refuses the apply, says the preview changed and recovers', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-orbit-development'
    const base = await readManifest(page, server, 'development', 2)
    const target: Manifest = structuredClone(base)
    target.version = 3
    target.work_items[0].fields.push({
      id: 'notes',
      kind: { type: 'text' },
      required: false,
      default: null,
    })
    await publish(page, server, target)

    await goto(page, server, `/projects/${projectId}/settings`)
    await expect(page.getByText(/A newer Development version is published/)).toBeVisible({ timeout: 20_000 })
    await expect(page.getByText(/^fingerprint /)).toBeVisible()

    await page.getByRole('button', { name: 'Upgrade workflow…' }).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByRole('checkbox').check()

    // A ledger write after the preview was reviewed changes its fingerprint;
    // the apply must be refused rather than confirming a stale comparison.
    const created = await page.request.post(
      `${server.base}/v1/authoritative/blueprints/items`,
      {
        headers: bearer(server),
        data: {
          id: 'wi-fingerprint-drift',
          project_id: projectId,
          kind: 'feature',
          properties: { title: 'Fingerprint drift', priority: 'low' },
        },
      },
    )
    expect(created.status(), await created.text()).toBe(200)

    await dialog.getByRole('button', { name: 'Apply upgrade' }).click()
    await expect(
      dialog.getByText(/The preview changed on the server, so this apply was refused/),
    ).toBeVisible({ timeout: 20_000 })
    expect(await bindingVersion(page, server, projectId)).toBe(2)

    // The refreshed preview no longer matches what was reviewed; reopening the
    // confirmation reviews the new fingerprint and the apply then succeeds.
    await expect(
      dialog.getByText(/The preview changed while this dialog was open/),
    ).toBeVisible({ timeout: 20_000 })
    await dialog.getByRole('button', { name: 'Cancel' }).click()
    await page.getByRole('button', { name: 'Upgrade workflow…' }).click()
    const retry = page.getByRole('dialog')
    await retry.getByRole('checkbox').check()
    await retry.getByRole('button', { name: 'Apply upgrade' }).click()

    await expect(page.locator('[data-upgrade-receipt]')).toBeVisible({ timeout: 20_000 })
    expect(await bindingVersion(page, server, projectId)).toBe(3)
  })

  test('a blocked preview disables apply and lists every blocker with its subjects', async ({
    page,
    workspace,
  }) => {
    const { server } = workspace
    const projectId = 'p-orbit-development'
    const base = await readManifest(page, server, 'development', 2)
    const target: Manifest = structuredClone(base)
    target.version = 3
    // Every seeded feature stores `priority`; removing the field is impossible
    // without a migration plan, and the server says so per item.
    target.work_items[0].fields = target.work_items[0].fields.filter(
      (field) => field.id !== 'priority',
    )
    await publish(page, server, target)

    await goto(page, server, `/projects/${projectId}/settings`)
    await expect(page.getByText(/A newer Development version is published/)).toBeVisible({ timeout: 20_000 })

    const blocker = page.locator('[data-upgrade-notice="field_removed_with_values"]')
    await expect(blocker).toBeVisible({ timeout: 20_000 })
    await expect(blocker).toContainText('priority')
    // Subjects are the live items, not a count.
    for (const itemId of [
      'wi-orbit-feature-backlog',
      'wi-orbit-feature-ready',
      'wi-orbit-feature-review',
    ]) {
      await expect(blocker.getByText(itemId)).toBeVisible()
    }

    await expect(page.locator('[data-upgrade-apply]')).toBeDisabled()
    await expect(
      page.getByText(/Apply is disabled until every blocker above is resolved/),
    ).toBeVisible()
    expect(await bindingVersion(page, server, projectId)).toBe(2)

    // A reload keeps the project on its pinned version: a preview is a read.
    await page.reload()
    await expect(page.locator('[data-upgrade-apply]')).toBeDisabled()
    expect(await bindingVersion(page, server, projectId)).toBe(2)
  })
})
