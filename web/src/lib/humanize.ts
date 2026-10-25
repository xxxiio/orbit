/**
 * Turns an internal identifier into display text: `project_health` → "Project
 * Health". Only for copy; never parse a value that has a real label map.
 */
export function humanizeIdentifier(value: string): string {
  const words = value.split(/[-_\s]+/).filter(Boolean)
  if (words.length === 0) return 'Unknown'
  return words
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
}

/**
 * A capability id as copy. Capability ids are dotted (`orbit.release.approve`),
 * which `humanizeIdentifier` would leave as one capitalised token; a person
 * reads the dotted segments as words.
 */
export function capabilityLabel(value: string): string {
  const words = value.split(/[._-]+/).filter(Boolean)
  if (words.length === 0) return value
  return words
    .map((word, index) =>
      index === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word,
    )
    .join(' ')
}
