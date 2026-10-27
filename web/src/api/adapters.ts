/**
 * Wire → UI mapping. One function per resource, and the only place internal
 * names are renamed ("workflow", "work item") and snake_case becomes camelCase.
 *
 * Nothing here applies business rules: values come from the server as sent.
 * The only normalisation is structural (absent vs null ids, see trap 1) and
 * boundary assertions for string enums the authority already validates.
 *
 * The mapping is split by domain — `projects`, `tasks`, `workspace`,
 * `planning-reads`, `workflows`, `attachments`, `agents`, `planning` — because
 * one file had grown to every resource in the product. This barrel keeps
 * `@/api/adapters` as the import path for one iteration; a caller that wants a
 * domain module directly may import it.
 */
export * from './adapters/projects'
export * from './adapters/tasks'
export * from './adapters/workspace'
export * from './adapters/planning-reads'
export * from './adapters/workflows'
export * from './adapters/attachments'
export * from './adapters/agents'
export * from './adapters/planning'
export * from './adapters/search'
export * from './adapters/saved-views'
