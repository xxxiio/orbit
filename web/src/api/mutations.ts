/**
 * Command hooks.
 *
 * The rules every command obeys (no optimistic writes, revision fences read at
 * mutation time, caller-allocated idempotency keys, honest `uncertain`
 * dispositions) are documented and implemented once in `./mutations/core`.
 *
 * The families are split by domain — `tasks`, `projects`, `milestones`,
 * `workflows`, `attachments`, `knowledge`, `agents`, `planning` — because this
 * file had grown to eighteen hooks and every new surface appended another. This
 * barrel keeps `@/api/mutations` (and therefore `@/hooks`) as the import path
 * for one iteration.
 */
export { newCommandId } from './ids'
export * from './mutations/core'
export * from './mutations/tasks'
export * from './mutations/projects'
export * from './mutations/milestones'
export * from './mutations/workflows'
export * from './mutations/attachments'
export * from './mutations/knowledge'
export * from './mutations/agents'
export * from './mutations/planning'
export * from './mutations/saved-views'
