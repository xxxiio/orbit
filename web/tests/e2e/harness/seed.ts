/**
 * E2E-facing wrapper around the reusable fixture in `web/tests/fixture/seed.ts`.
 *
 * The fixture module is transport-agnostic so the same data can seed a
 * long-lived server later (dogfooding); here it is wired to one harness server
 * using that server's enrolled device credentials.
 */
import {
  seedWorkspace as seedWithTransport,
  bearerTransport,
  type SeedOptions,
  type WorkspaceFixture,
} from '../../fixture/seed'
import type { E2EServer } from './server'

export type { WorkspaceFixture, SeedOptions }

/**
 * The clock every acceptance workspace is seeded against: today at 09:00 in the
 * harness's local time zone.
 *
 * Deadlines are relative to `nowMs`, and the product decides "overdue" and
 * "due today" from the browser's clock. Anchoring both to one fixed moment of
 * the current day makes the seeded workspace and the rendered sections agree at
 * any real wall-clock time — a `now + 4h` deadline would stop being "today" for
 * a suite that runs after 20:00, and the fixture would silently disagree with
 * the UI. The `workspace` fixture also pins the browser clock to this value.
 */
export function fixtureNowMs(now = Date.now()): number {
  const anchor = new Date(now)
  anchor.setHours(9, 0, 0, 0)
  return anchor.getTime()
}

/** Seed the isolated workspace for one harness server, through its real API. */
export function seedWorkspace(
  server: E2EServer,
  options: SeedOptions = {},
): Promise<WorkspaceFixture> {
  return seedWithTransport(
    bearerTransport({ base: server.base, deviceId: server.deviceId, token: server.token }),
    { machineRef: server.deviceId, nowMs: fixtureNowMs(), ...options },
  )
}
