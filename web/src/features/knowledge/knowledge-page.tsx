/**
 * The Knowledge workspace: folder/note tree, inventory search and one note.
 *
 * Every read is a server read (`knowledge-tree`, `markdown-document`,
 * `markdown-search`). There is no second note store and no rich-text editor:
 * the note body is shown with the shared safe Markdown renderer, and edits are
 * plain Markdown bytes posted as a fenced `Update`.
 */
import { useEffect, useMemo, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import type { KnowledgeDocumentInfo } from '@/api/contracts'
import type { ApiError } from '@/api/errors'
import type { KnowledgeNode } from '@/api/contracts'
import {
  useDocumentTitle,
  useKnowledgeTree,
  useMarkdownInventory,
  useMarkdownSearch,
  userMessage,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
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
import { cn } from '@/components/ui/utils'
import { noteCountLabel, noteHref, notePathFromSplat } from './knowledge-model'
import { NewNoteDialog } from './note-create-dialog'
import { NoteView } from './note-view'

function collectFolders(node: KnowledgeNode | undefined, into: string[] = []): string[] {
  if (!node || node.kind !== 'folder') return into
  if (node.path.length > 0) into.push(node.path)
  for (const child of node.children) collectFolders(child, into)
  return into
}

function countNotes(node: KnowledgeNode): number {
  if (node.kind === 'note') return 1
  return node.children.reduce((total, child) => total + countNotes(child), 0)
}

function NoteLink({
  path,
  title,
  current,
  depth,
}: {
  path: string
  title: string
  current: boolean
  depth: number
}) {
  return (
    <li>
      <Link
        to={noteHref(path)}
        data-note-path={path}
        aria-current={current ? 'page' : undefined}
        className={cn(
          'flex items-center gap-2 rounded-md py-1 pr-2 text-sm transition-colors',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
          current
            ? 'bg-accent-soft font-medium text-accent-soft-ink'
            : 'text-ink-soft hover:bg-surface-hover/60 hover:text-ink',
        )}
        style={{ paddingLeft: `${0.5 + depth * 0.85}rem` }}
        title={path}
      >
        <BookIcon size={14} className="shrink-0 opacity-70" />
        <span className="min-w-0 flex-1 truncate">{title}</span>
      </Link>
    </li>
  )
}

function TreeNode({
  node,
  currentPath,
  collapsed,
  toggle,
}: {
  node: KnowledgeNode
  currentPath: string
  collapsed: Set<string>
  toggle: (path: string) => void
}) {
  if (node.kind === 'note') {
    return (
      <NoteLink
        path={node.path}
        title={node.name.replace(/\.md$/i, '')}
        current={node.path === currentPath}
        depth={node.path.split('/').length - 1}
      />
    )
  }

  const isRoot = node.path.length === 0
  const isCollapsed = collapsed.has(node.path)
  const notes = countNotes(node)
  return (
    <li>
      {isRoot ? null : (
        <button
          type="button"
          onClick={() => toggle(node.path)}
          aria-expanded={!isCollapsed}
          className="flex w-full items-center gap-1.5 rounded-md py-1 pr-2 text-left text-sm font-medium text-ink-soft transition-colors hover:bg-surface-hover/60 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          style={{ paddingLeft: `${0.5 + (node.path.split('/').length - 1) * 0.85}rem` }}
        >
          {isCollapsed ? (
            <ChevronRightIcon size={13} className="shrink-0 text-ink-muted" />
          ) : (
            <ChevronDownIcon size={13} className="shrink-0 text-ink-muted" />
          )}
          <FolderIcon size={14} className="shrink-0 text-ink-muted" />
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
          <span className="shrink-0 text-2xs tabular-nums text-ink-faint">{notes}</span>
        </button>
      )}
      {isCollapsed ? null : (
        <ul className={cn('flex flex-col gap-0.5', !isRoot && 'mt-0.5')}>
          {node.children.map((child) => (
            <TreeNode
              key={child.path}
              node={child}
              currentPath={currentPath}
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
  const navigate = useNavigate()
  const currentPath = notePathFromSplat(params['*'])
  const tree = useKnowledgeTree()
  const inventory = useMarkdownInventory()

  const [search, setSearch] = useState('')
  const [debounced, setDebounced] = useState('')
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set())
  const [createOpen, setCreateOpen] = useState(false)

  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(search.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [search])

  const results = useMarkdownSearch(debounced)
  const folders = useMemo(() => collectFolders(tree.data), [tree.data])

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

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Workspace"
        title="Knowledge"
        description="Notes are Markdown files on the server. Their stable id lives in the frontmatter, so moving a file never breaks a [[link]]."
        actions={
          <Button leadingIcon={<PlusIcon size={15} />} onClick={() => setCreateOpen(true)}>
            New note
          </Button>
        }
      />

      <div className="grid min-w-0 items-start gap-5 lg:grid-cols-[19rem_minmax(0,1fr)]">
        <Panel
          className={cn(
            'min-w-0 overflow-hidden lg:sticky lg:top-4',
            currentPath ? 'order-2 lg:order-1' : 'order-1',
          )}
        >
          <PanelHeader
            title="Vault"
            description={tree.data ? noteCountLabel(countNotes(tree.data)) : 'Reading folders…'}
            actions={
              <Button
                variant="quiet"
                size="sm"
                onClick={refreshTree}
                loading={tree.isFetching}
                aria-label="Refresh the knowledge tree"
              >
                Refresh
              </Button>
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
              <nav aria-label="Knowledge tree" className="max-h-[65vh] overflow-y-auto">
                <ul className="flex flex-col gap-0.5">
                  <TreeNode
                    node={tree.data}
                    currentPath={currentPath}
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
            <NoteView key={currentPath} path={currentPath} onRefreshTree={refreshTree} />
          ) : (
            <Panel>
              <EmptyState
                icon={<BookIcon size={21} />}
                title="Choose a note, or create one"
                description="The tree lists every Markdown file in the vault. A note opens from its path; its links resolve by the stable id in the frontmatter."
                action={
                  <Button leadingIcon={<PlusIcon size={15} />} onClick={() => setCreateOpen(true)}>
                    Create a note
                  </Button>
                }
                secondaryAction={
                  tree.data && countNotes(tree.data) === 0 ? (
                    <Badge tone="neutral">The vault is empty</Badge>
                  ) : null
                }
                className="min-h-[20rem]"
              />
            </Panel>
          )}
        </div>
      </div>

      {createOpen ? (
        <NewNoteDialog
          folders={folders}
          onClose={() => setCreateOpen(false)}
          onCreated={(path) => {
            void inventory.refetch()
            void tree.refetch()
            navigate(noteHref(path))
          }}
          onRefreshTree={refreshTree}
        />
      ) : null}
    </div>
  )
}
