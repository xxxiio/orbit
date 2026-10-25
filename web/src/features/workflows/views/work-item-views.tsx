/**
 * Work-item surfaces — a compatibility barrel.
 *
 * The four renderers moved into `./work-item/{board,table,list,detail}` and
 * share `./work-item/shared` plus the surface kit (`@/components/surface`).
 * This module keeps the old import path working for one iteration.
 */
export { WorkItemBoardView } from './work-item/board'
export { WorkItemDetailView } from './work-item/detail'
export { WorkItemListView } from './work-item/list'
export { WorkItemTableView } from './work-item/table'
