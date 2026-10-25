/**
 * The task drawer, split along its section boundaries.
 *
 * The generic editors it composes (`InlineTitle`, `EditGuardProvider`/
 * `useDirtyGuard`, `PropertyField`/`PropertySelect`) live in
 * `components/forms` because a future work-item drawer uses them too; this
 * module only exports the task-typed shell.
 */
export { TaskDetailDrawer } from './task-drawer'
