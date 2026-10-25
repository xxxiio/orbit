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

/** Seed the isolated workspace for one harness server, through its real API. */
export function seedWorkspace(
  server: E2EServer,
  options: SeedOptions = {},
): Promise<WorkspaceFixture> {
  return seedWithTransport(
    bearerTransport({ base: server.base, deviceId: server.deviceId, token: server.token }),
    { machineRef: server.deviceId, ...options },
  )
}
