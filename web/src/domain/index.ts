/**
 * Framework-free product rules shared across features.
 *
 * Everything here is plain TypeScript: no React, no TanStack Query, no UI
 * imports. If a rule needs a component or a server read it belongs in a
 * feature; if two features need the same rule, it belongs here so neither owns
 * the other's vocabulary.
 */
export {
  milestoneStatusLabel,
  milestoneStatusLabels,
  projectStatusLabel,
  projectStatusLabels,
} from './project-status'
export { isTaskCompleted, isTaskOpen } from './project-progress'
export { formatTimeOfDay } from './datetime'
export { workflowCopy, workflowText } from './workflow-copy'
