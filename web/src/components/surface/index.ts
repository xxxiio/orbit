/**
 * The product surface kit: panel chrome, the resource read guard and the
 * board/table chrome every resource surface shares.
 *
 * It lives under `components/` because it is cross-cutting UI, not workflow
 * vocabulary: a workflow view, a knowledge panel and an agent-jobs panel must
 * render the same panel, the same empty/refusal states and the same limit note
 * without any of them importing another feature.
 */
export {
  ComponentPanel,
  DeclaredActionsNote,
  InlineReadError,
  PanelRetry,
  ProvenanceNote,
  RefusalNote,
  UnavailablePanel,
} from './panel'
export {
  DeclaredActionsFooter,
  ResourceReadPrelude,
  SurfaceFooter,
  filterSortLimit,
  type ResourceReadState,
  type ResourceRows,
} from './resource'
export {
  BoardCard,
  BoardColumn,
  BoardColumnEmpty,
  BoardKeyboardHint,
  BoardScrollRegion,
  useBoardKeyboard,
  type BoardGroup,
} from './board'
export {
  ColumnNotice,
  TableHead,
  TableHeadCell,
  TableScrollRegion,
  limitNote,
} from './table'
