/**
 * Calendar and clock formatting shared by every feature.
 *
 * These are presentation rules, not task rules: a schedule block, a planning
 * window and a deadline must all read the same time of day. Keeping them in
 * `domain/` means the task kernel does not own the vocabulary of time.
 */

/** Clock time only, for schedule blocks and planning windows. */
const timeFormatter = new Intl.DateTimeFormat(undefined, {
  hour: 'numeric',
  minute: '2-digit',
})

export function formatTimeOfDay(ms: number): string {
  return timeFormatter.format(new Date(ms))
}
