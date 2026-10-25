/**
 * The work-item sheet is URL state (`?item=<id>`), like the task drawer: a
 * refresh keeps it open, Back closes it, and a link can point at one item.
 * The query parameter is namespaced away from `?task=` so both drawers can
 * coexist in the same workspace.
 */
import { useResourceDrawer } from '@/hooks/use-resource-drawer'

export function useWorkItemDrawer() {
  const drawer = useResourceDrawer({
    param: 'item',
    rowAttribute: 'data-work-item-key',
    actionAttribute: 'data-work-item-action',
  })
  return { itemId: drawer.id, openItem: drawer.open, closeItem: drawer.close }
}
