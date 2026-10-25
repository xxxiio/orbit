/**
 * The task drawer, split along its section boundaries.
 *
 * Reusable by a future work-item drawer: `EditGuardProvider`/`useDirtyGuard`,
 * `Section`, `InlineTitle`, `LabelsEditor`, `DescriptionEditor` and
 * `PropertyField`/`PropertySelect` are all resource-agnostic.
 */
export { TaskDetailDrawer } from './task-drawer'
export { DescriptionEditor } from './description-editor'
export { InlineTitle } from './inline-title'
export { LabelsEditor } from './labels-editor'
export { FieldError, SavingNote, Section } from './section'
export { EditGuardProvider, useDirtyGuard, type EditorHandle } from './use-edit-guard'
export { PropertyField, PropertySelect } from '@/components/forms'
