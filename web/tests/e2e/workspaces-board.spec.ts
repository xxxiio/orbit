/**
 * I2 acceptance — the task board.
 *
 * Board validity is the server's decision; these specs check that the UI sends
 * the command, shows the answer and never moves a card on its own.
 */
import { expect, test } from './harness/fixtures'
import { goto } from './harness/session'
import { boardColumn, taskDetail } from './harness/workspace-reads'

test.describe('board', () => {
  test('an accepted keyboard move follows the server and survives a reload', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) => candidate.projectId === 'p-basic' && candidate.status === 'inbox',
    )
    expect(task, 'the fixture needs an inbox task in p-basic').toBeTruthy()

    await goto(page, server, '/projects/p-basic/board')
    const inbox = boardColumn(page, /^Inbox column/)
    const planned = boardColumn(page, /^Planned column/)

    const card = inbox.getByRole('button', { name: task!.title, exact: true })
    await expect(card).toBeVisible()

    // Keyboard parity: the card is one tab stop and h/j/k/l move the cursor.
    await card.focus()
    await page.keyboard.press('l')
    await expect(page.locator('button[data-board-action]:focus')).not.toHaveCount(0)

    await goto(page, server, '/projects/p-basic/board')
    await card.focus()
    await page.keyboard.press('m')
    const menu = page.getByRole('menu')
    await expect(menu).toBeVisible()
    // Inbox's domain edges only: planned, ready, deferred, cancelled.
    await expect(menu.getByRole('menuitem')).toHaveCount(4)
    await expect(menu.getByRole('menuitem', { name: 'Done', exact: true })).toHaveCount(0)
    await page.getByRole('menuitem', { name: 'Planned', exact: true }).click()

    // The card is in the new column because the server moved it, not because
    // the client did.
    await expect(planned.getByRole('button', { name: task!.title, exact: true })).toBeVisible()
    await expect(inbox.getByRole('button', { name: task!.title, exact: true })).toHaveCount(0)
    expect((await taskDetail(page, server, task!.id)).status).toBe('planned')

    await page.reload()
    await expect(page.getByRole('heading', { name: 'Basic', level: 1 })).toBeVisible()
    await expect(
      boardColumn(page, /^Planned column/).getByRole('button', { name: task!.title, exact: true }),
    ).toBeVisible()
  })

  test('the Move menu does not offer an edge the domain refuses', async ({ page, workspace }) => {
    const { server, fixture } = workspace
    const task = fixture.tasks.find(
      (candidate) => candidate.projectId === 'p-basic' && candidate.status === 'inbox',
    )
    expect(task, 'the fixture needs an inbox task in p-basic').toBeTruthy()

    await goto(page, server, '/projects/p-basic/board')
    const inbox = boardColumn(page, /^Inbox column/)
    await inbox
      .getByRole('button', { name: `Move ${task!.title} to another status` })
      .click()

    // `inbox → done` is refused by the domain, so the menu never offers it and
    // there is no client-side rejection round trip to make.
    const menu = page.getByRole('menu')
    await expect(menu).toBeVisible()
    await expect(menu.getByRole('menuitem', { name: 'Done', exact: true })).toHaveCount(0)
    await expect(menu.getByRole('menuitem', { name: 'In progress', exact: true })).toHaveCount(0)
    await expect(menu.getByRole('menuitem', { name: 'Planned', exact: true })).toBeVisible()
    await page.keyboard.press('Escape')

    // The card never moved and nothing was written.
    await expect(inbox.getByRole('button', { name: task!.title, exact: true })).toBeVisible()
    expect((await taskDetail(page, server, task!.id)).status).toBe('inbox')
  })
})
