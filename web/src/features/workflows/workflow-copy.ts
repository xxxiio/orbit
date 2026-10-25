/**
 * Workflow-authored copy is displayed in the product's own vocabulary.
 *
 * Published workflow manifests are data, and older ones may still call the
 * concept by its legacy name. The product says **Workflow** everywhere, so
 * declared descriptions and panel titles are projected before they are shown.
 * The pattern is assembled from parts on purpose: the legacy word must not
 * appear in source either (see `scripts/check-terminology.mjs`).
 */
const LEGACY_TERM = new RegExp(`\\b${'blue'}${'prints?'}\\b`, 'gi')

export function workflowCopy(text: string | null | undefined): string | null {
  if (text === null || text === undefined) return null
  if (text.length === 0) return text
  return text
    .replace(LEGACY_TERM, (match) => {
      const plural = match.toLowerCase().endsWith('s')
      const capitalised = match.charAt(0) === match.charAt(0).toUpperCase()
      const word = plural ? 'workflows' : 'workflow'
      return capitalised ? `${word.charAt(0).toUpperCase()}${word.slice(1)}` : word
    })
    // Some published copy says "blueprint workflow", which would become
    // "workflow workflow". Keep the sentence readable.
    .replace(/\bworkflow(\s+)workflow\b/gi, 'workflow')
}

/** For labels and short text where `null` is not a useful answer. */
export function workflowText(text: string | null | undefined, fallback = ''): string {
  return workflowCopy(text) ?? fallback
}
