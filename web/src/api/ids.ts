/**
 * Idempotency/identity keys.
 *
 * Kept in its own module so endpoints can build a `command_id` without
 * importing the hook layer (which would be a cycle). `mutations.ts` re-exports
 * `newCommandId` so every existing import keeps working.
 */

/** Idempotency key: 1–128 characters. `crypto.randomUUID()` is 36. */
export function newCommandId(): string {
  const cryptoApi = globalThis.crypto
  if (cryptoApi && typeof cryptoApi.randomUUID === 'function') {
    return cryptoApi.randomUUID()
  }
  return `cmd-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}
