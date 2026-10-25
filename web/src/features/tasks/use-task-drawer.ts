import { useResourceDrawer } from '@/hooks/use-resource-drawer'

/**
 * The task drawer is a URL state (`?task=<id>`). The rules live in
 * `useResourceDrawer`; this module only names the task's parameter and row
 * attributes.
 */
export function useTaskDrawer() {
  const drawer = useResourceDrawer({
    param: 'task',
    rowAttribute: 'data-row-key',
    actionAttribute: 'data-row-action',
  })
  return { taskId: drawer.id, openTask: drawer.open, closeTask: drawer.close }
}
