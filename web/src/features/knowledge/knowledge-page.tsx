/**
 * The Knowledge workspace: folder/note tree, inventory search, one note, and a
 * folder as a real destination (`?folder=<path>`).
 *
 * Every read is a server read (`knowledge-tree`, `markdown-document`,
 * `markdown-search`). There is no second note store and no rich-text editor:
 * the note body is shown with the shared safe Markdown renderer, and edits are
 * plain Markdown bytes posted as a fenced `Update`.
 *
 * Folder create, rename and move are commands over real directories. A folder
 * move changes every note path beneath it, so the whole knowledge read family is
 * invalidated and an open note is followed to its new path.
 */
import { useEffect, useMemo, useState } from 'react'
import { Link, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import type { KnowledgeDocumentInfo, KnowledgeNode } from '@/api/contracts'
import {
  useDocumentTitle,
  useKnowledgeTree,
  useMarkdownInventory,
  useMarkdownSearch,
  userMessage,
  type ApiError,
} from '@/hooks'
import { Button } from '@/components/ui/button'
import { ErrorState } from '@/components/ui/error-state'
import {
  AlertCircleIcon,
  BookIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  FolderIcon,
  PlusIcon,
  SearchIcon,
} from '@/components/ui/icons'
import { Input } from '@/components/ui/input'
import { PageHeader } from '@/components/ui/page-header'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Skeleton, SkeletonText } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { cn } from '@/components/ui/utils'
import type { FolderOption } from './folder-model'
import {
  VAULT_ROOT,
  allFolderPaths,
  countNotesIn,
  folderBreadcrumb,
  folderOptions,
  isWithinFolder,
  noteFolder,
  rebasePath,
} from './folder-model'
import { FolderView } from './folder-view'
import { NewFolderDialog } from './folder/folder-create-dialog'
import { FolderMoveDialog } from './folder/folder-move-dialog'
import { folderHref, noteCountLabel, noteHref, notePathFromSplat } from './knowledge-model'
import { NewNoteDialog } from './note-create-dialog'
import { NoteView } from './note-view'

function TreeNode({
  node,
  currentPath,
  selectedFolder,
  collapsed,
  toggle,
}: {
  node: KnowledgeNode
  currentPath: string
  selectedFolder: string
  collapsed: Set<string>
  toggle: (path: string) => void
}) {
  if (node.kind === 'note') {
    const current = node.path === currentPath
    return (
      <li>
        <Link
          to={noteHref(node.path)}
          data-note-path={node.path}
          aria-current={current ? 'page' : undefined}
          className={cn(
            'flex items-center gap-2 rounded-md py-1 pr-2 text-sm transition-colors',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
            current
              ? 'bg-accent-soft font-medium text-accent-soft-ink'
              : 'text-ink-soft hover:bg-surface-hover/60 hover:text-ink',
          )}
          style={{ paddingLeft: `${0.5 + (node.path.split('/').length - 1) * 0.85}rem` }}
          title={node.path}
        >
          <BookIcon size={14} className="shrink-0 opacity-70" />
          <span className="min-w-0 flex-1 truncate">{node.name.replace(/\.md$/i, '')}</span>
        </Link>
      </li>
    )
  }

  const isRoot = node.path.length === 0
  const isCollapsed = collapsed.has(node.path)
  const notes = countNotesIn(node)
  const selected = !currentPath && node.path === selectedFolder
  const depth = Math.max(0, node.path.split('/').length - 1)

  return (
    <li data-folder-row={node.path}>
      {isRoot ? null : (
        <div
          className="flex items-center gap-0.5"
          style={{ paddingLeft: `${0.25 + depth * 0.85}rem` }}
        >
          <button
            type="button"
            onClick={() => toggle(node.path)}
            aria-expanded={!isCollapsed}
            aria-label={`${isCollapsed ? 'Expand' : 'Collapse'} the folder ${node.name}`}
            data-folder-expand={node.path}
            className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-muted transition-colors hover:bg-surface-hover hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          >
            {isCollapsed ? (
              <ChevronRightIcon size={13} className="shrink-0" />
            ) : (
              <ChevronDownIcon size={13} className="shrink-0" />
            )}
          </button>
          <Link
            to={folderHref(node.path)}
            aria-current={selected ? 'page' : undefined}
            data-folder-path={node.path}
            className={cn(
              'flex min-w-0 flex-1 items-center gap-1.5 rounded-md py-1 pr-2 text-sm font-medium transition-colors',
              'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
              selected
                ? 'bg-accent-soft text-accent-soft-ink'
                : 'text-ink-soft hover:bg-surface-hover/60 hover:text-ink',
            )}
          >
            <FolderIcon size={14} className="shrink-0 text-ink-muted" />
            <span className="min-w-0 flex-1 truncate">{node.name}</span>
            <span className="shrink-0 text-2xs tabular-nums text-ink-faint">{notes}</span>
          </Link>
        </div>
      )}
      {isCollapsed ? null : (
        <ul className={cn('flex flex-col gap-0.5', !isRoot && 'mt-0.5')}>
          {node.children.map((child) => (
            <TreeNode
              key={child.path}
              node={child}
              currentPath={currentPath}
              selectedFolder={selectedFolder}
              collapsed={collapsed}
              toggle={toggle}
            />
          ))}
        </ul>
      )}
    </li>
  )
}

function SearchResults({
  query,
  results,
  pending,
  error,
  retry,
  retrying,
  currentPath,
}: {
  query: string
  results: KnowledgeDocumentInfo[]
  pending: boolean
  error: ApiError | null
  retry: () => void
  retrying: boolean
  currentPath: string
}) {
  if (error) {
    return (
      <ErrorState
        compact
        title="Search could not run"
        description={userMessage(error)}
        onRetry={retry}
        retrying={retrying}
      />
    )
  }
  if (pending && results.length === 0) {
    return (
      <div aria-busy="true" className="px-1 py-2">
        <SkeletonText lines={3} />
      </div>
    )
  }
  if (results.length === 0) {
    return (
      <p className="px-1 py-2 text-xs leading-5 text-ink-muted">
        No note title or body contains “{query}”. Try another word from the note.
      </p>
    )
  }
  return (
    <ul aria-label="Search results" className="flex flex-col gap-0.5">
      {results.map((note) => (
        <li key={note.id}>
          <Link
            to={noteHref(note.path)}
            aria-current={note.path === currentPath ? 'page' : undefined}
            className={cn(
              'flex flex-col rounded-md px-2 py-1.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
              note.path === currentPath
                ? 'bg-accent-soft text-accent-soft-ink'
                : 'hover:bg-surface-hover/60',
            )}
          >
            <span className="truncate text-sm font-medium">{note.title}</span>
            <span className="truncate font-mono text-2xs text-ink-muted">{note.path}</span>
          </Link>
        </li>
      ))}
    </ul>
  )
}

export function KnowledgePage() {
  useDocumentTitle('Knowledge')
  const params = useParams()
  const [searchParams, setSearchParams] = useSearchParams()
  const navigate = useNavigate()
  const { toast } = useToast()
  const currentPath = notePathFromSplat(params['*'])
  const selectedFolder = searchParams.get('folder') ?? VAULT_ROOT
  const contextProjectId = searchParams.get('project') ?? undefined
  const contextTaskId = searchParams.get('task') ?? undefined
  const tree = useKnowledgeTree()
  const inventory = useMarkdownInventory()

  const [search, setSearch] = useState('')
  const [debounced, setDebounced] = useState('')
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set())
  const [createOpen, setCreateOpen] = useState(false)
  const [newFolderOpen, setNewFolderOpen] = useState(false)
  const [folderMoveOpen, setFolderMoveOpen] = useState<string | null>(null)

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(search.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [search])

  const results = useMarkdownSearch(debounced)
  const options: FolderOption[] = useMemo(() => folderOptions(tree.data), [tree.data])
  const folderPaths = useMemo(() => allFolderPaths(tree.data), [tree.data])
  const folderMissing =
    selectedFolder !== VAULT_ROOT && tree.data !== undefined && !folderPaths.includes(selectedFolder)
  const moveOptions = useMemo(
    () => folderOptions(tree.data, { exclude: folderMoveOpen }),
    [tree.data, folderMoveOpen],
  )
  // The trail shows where the person is, whether that is a folder or a note.
  const locationFolder = currentPath ? noteFolder(currentPath) : selectedFolder
  const crumbs = useMemo(() => folderBreadcrumb(locationFolder), [locationFolder])

  const toggle = (path: string) => {
    setCollapsed((current) => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }

  const refreshTree = () => {
    void tree.refetch()
    void inventory.refetch()
  }

  const openFolder = (folder: string) => navigate(folderHref(folder))

  /**
   * `?task=` on a knowledge route is the task this note is being linked to. It
   * is dropped from the address without leaving the note or touching its bytes;
   * every other parameter (`?project=`, `?folder=`) is kept as it was.
   */
  const clearTaskContext = () => {
    setSearchParams(
      (current) => {
        if (!current.has('task')) return current
        const next = new URLSearchParams(current)
        next.delete('task')
        return next
      },
      { replace: true },
    )
  }

  /**
   * A folder move changes every path under it. The tree is re-read (the mutation
   * already invalidated the whole knowledge family), and an open note or folder
   * that lived inside the moved folder follows it.
   */
  const afterFolderMove = (from: string, to: string) => {
    refreshTree()
    toast({ tone: 'success', title: 'Folder moved', description: to })
    if (currentPath) {
      const nextNote = rebasePath(currentPath, from, to)
      if (nextNote) {
        navigate(noteHref(nextNote))
        return
      }
      return
    }
    if (isWithinFolder(selectedFolder, from)) {
      const nextFolder = rebasePath(selectedFolder, from, to)
      if (nextFolder) openFolder(nextFolder)
    }
  }

  const noteCount = tree.data ? countNotesIn(tree.data) : 0

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Workspace"
        title="Knowledge"
        description="Notes are Markdown files in folders on the server. Their stable id lives in the frontmatter, so moving a file never breaks a [[link]]."
        actions={
          <Button
            leadingIcon={<PlusIcon size={15} />}
            onClick={() => setCreateOpen(true)}
            data-action="new-note"
          >
            New note
          </Button>
        }
      />

      <nav aria-label="Folder location" data-breadcrumb className="flex flex-wrap items-center gap-1 text-xs">
        {crumbs.map((crumb, index) => {
          const last = index === crumbs.length - 1
          return (
            <span key={crumb.path || 'root'} className="flex items-center gap-1">
              {index > 0 ? (
                <span aria-hidden="true" className="text-ink-faint">
                  /
                </span>
              ) : null}
              <Link
                to={folderHref(crumb.path)}
                data-breadcrumb-path={crumb.path}
                aria-current={last && !currentPath ? 'page' : undefined}
                className={cn(
                  'rounded-sm px-1 py-0.5 underline-offset-2 transition-colors',
                  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                  last && !currentPath
                    ? 'font-medium text-ink'
                    : 'text-ink-muted hover:text-ink hover:underline',
                )}
              >
                {crumb.name}
              </Link>
            </span>
          )
        })}
        {currentPath ? (
          <span className="flex items-center gap-1">
            <span aria-hidden="true" className="text-ink-faint">
              /
            </span>
            <span className="px-1 py-0.5 font-medium text-ink" data-note-location>
              {currentPath.split('/').pop()}
            </span>
          </span>
        ) : null}
      </nav>

      <div className="grid min-w-0 items-start gap-5 lg:grid-cols-[19rem_minmax(0,1fr)]">
        <Panel
          className={cn(
            'min-w-0 overflow-hidden lg:sticky lg:top-4',
            currentPath ? 'order-2 lg:order-1' : 'order-1',
          )}
        >
          <PanelHeader
            title="Vault"
            description={tree.data ? noteCountLabel(noteCount) : 'Reading folders…'}
            actions={
              <div className="flex items-center gap-1">
                {/*
                  The visible text is the accessible name. A different
                  `aria-label` ("Create a folder") would hide the words a sighted
                  person reads from a screen reader and voice control
                  (axe `label-content-name-mismatch`); the panel heading already
                  says which vault this button belongs to.
                */}
                <Button
                  variant="quiet"
                  size="sm"
                  onClick={() => setNewFolderOpen(true)}
                  data-action="new-folder"
                >
                  New folder
                </Button>
                <Button
                  variant="quiet"
                  size="sm"
                  onClick={refreshTree}
                  loading={tree.isFetching}
                  aria-label="Refresh the knowledge tree"
                >
                  Refresh
                </Button>
              </div>
            }
          />
          <PanelBody className="px-2 py-3">
            <div className="px-1 pb-2">
              <Input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search titles and bodies…"
                aria-label="Search notes"
                data-search-input
                leadingIcon={<SearchIcon size={14} />}
                inputSize="md"
              />
            </div>

            {tree.isPending && !tree.data ? (
              <div aria-busy="true" className="flex flex-col gap-2 px-1 py-2">
                {Array.from({ length: 5 }, (_, index) => (
                  <Skeleton key={index} className="h-5" />
                ))}
              </div>
            ) : tree.error && !tree.data ? (
              <ErrorState
                compact
                title="The vault could not be read"
                description={userMessage(tree.error)}
                onRetry={() => void tree.refetch()}
                retrying={tree.isFetching}
              />
            ) : debounced.length > 0 ? (
              <SearchResults
                query={debounced}
                results={results.data ?? []}
                pending={results.isPending}
                error={results.error && !results.data ? results.error : null}
                retry={() => void results.refetch()}
                retrying={results.isFetching}
                currentPath={currentPath}
              />
            ) : tree.data ? (
              <nav aria-label="Knowledge tree" className="max-h-[65vh] overflow-y-auto" data-knowledge-tree>
                <ul className="flex flex-col gap-0.5">
                  <TreeNode
                    node={tree.data}
                    currentPath={currentPath}
                    selectedFolder={selectedFolder}
                    collapsed={collapsed}
                    toggle={toggle}
                  />
                </ul>
              </nav>
            ) : null}

            {inventory.error && !inventory.data ? (
              <p className="mt-3 flex items-start gap-1.5 px-1 text-2xs leading-4 text-warning-ink">
                <AlertCircleIcon size={12} className="mt-0.5 shrink-0" />
                Note identities could not be read, so relation links stay plain text.
              </p>
            ) : null}
          </PanelBody>
        </Panel>

        <div className={cn('min-w-0', currentPath && 'order-1 lg:order-2')}>
          {currentPath ? (
            <NoteView
              key={currentPath}
              path={currentPath}
              onRefreshTree={refreshTree}
              folderOptions={options}
              onClearTaskContext={clearTaskContext}
              context={
                contextProjectId || contextTaskId
                  ? { projectId: contextProjectId, taskId: contextTaskId }
                  : undefined
              }
            />
          ) : (
            <FolderView
              folder={selectedFolder}
              tree={tree.data}
              loading={tree.isFetching}
              error={tree.error}
              missing={folderMissing}
              onRetry={refreshTree}
              onNewNote={() => setCreateOpen(true)}
              onNewFolder={() => setNewFolderOpen(true)}
              onRenameOrMove={() => setFolderMoveOpen(selectedFolder)}
            />
          )}
        </div>
      </div>

      {createOpen ? (
        <NewNoteDialog
          options={options}
          initialFolder={selectedFolder}
          onClose={() => setCreateOpen(false)}
          onCreated={(path) => {
            refreshTree()
            navigate(noteHref(path))
          }}
          onRefreshTree={refreshTree}
        />
      ) : null}

      {newFolderOpen ? (
        <NewFolderDialog
          options={options}
          initialParent={selectedFolder}
          onClose={() => setNewFolderOpen(false)}
          onCreated={(path) => {
            refreshTree()
            openFolder(path)
            toast({ tone: 'success', title: 'Folder ready', description: path })
          }}
          onRefreshTree={refreshTree}
        />
      ) : null}

      {folderMoveOpen !== null ? (
        <FolderMoveDialog
          path={folderMoveOpen}
          options={moveOptions}
          onClose={() => setFolderMoveOpen(null)}
          onMoved={afterFolderMove}
          onRefreshTree={refreshTree}
        />
      ) : null}
    </div>
  )
}
