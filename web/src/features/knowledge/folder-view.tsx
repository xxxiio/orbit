/**
 * A folder as a real destination: its child folders, the notes directly inside
 * it, and the actions that belong to it (new note, new folder, rename or move).
 *
 * The page keeps this address in the URL (`?folder=…`), so a folder can be
 * bookmarked, shared and returned to with Back. Nothing here is a highlight: a
 * folder row is a link and the panel always describes what it is showing.
 */
import { Link } from 'react-router-dom'
import type { ApiError } from '@/hooks'
import { userMessage } from '@/hooks'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { ErrorState } from '@/components/ui/error-state'
import { BookIcon, FolderIcon, PlusIcon } from '@/components/ui/icons'
import { Panel, PanelBody, PanelHeader } from '@/components/ui/panel'
import { Skeleton } from '@/components/ui/skeleton'
import type { KnowledgeNode } from '@/api/contracts'
import {
  childFolders,
  countNotesIn,
  findFolder,
  folderName,
  notesInFolder,
  VAULT_ROOT,
} from './folder-model'
import { folderHref, noteCountLabel, noteHref, noteTitleFromPath } from './knowledge-model'

export interface FolderViewProps {
  folder: string
  tree: KnowledgeNode | undefined
  loading: boolean
  error: ApiError | null
  /** The tree loaded and the address names no folder in it. */
  missing: boolean
  onRetry: () => void
  onNewNote: () => void
  onNewFolder: () => void
  onRenameOrMove: () => void
}

export function FolderView({
  folder,
  tree,
  loading,
  error,
  missing,
  onRetry,
  onNewNote,
  onNewFolder,
  onRenameOrMove,
}: FolderViewProps) {
  if (loading && !tree) {
    return (
      <Panel aria-busy="true" aria-label="Loading folder">
        <div className="border-b border-line px-5 py-4">
          <Skeleton className="h-3 w-16" />
          <Skeleton className="mt-3 h-6 w-48" />
        </div>
        <div className="flex flex-col gap-2 px-5 py-4">
          {Array.from({ length: 4 }, (_, index) => (
            <Skeleton key={index} className="h-5" />
          ))}
        </div>
      </Panel>
    )
  }

  if (error && !tree) {
    return (
      <Panel>
        <ErrorState
          title="This folder could not be read"
          description={userMessage(error)}
          onRetry={onRetry}
          retrying={loading}
        />
      </Panel>
    )
  }

  if (missing) {
    return (
      <Panel>
        <EmptyState
          icon={<FolderIcon size={21} />}
          title="This folder is not in the vault"
          description={
            <>
              The vault has no folder at{' '}
              <span className="font-mono text-ink-soft">{folder}</span>. It may have been renamed
              or moved from another device.
            </>
          }
          action={
            <Button asChild variant="secondary" size="sm">
              <Link to={folderHref(VAULT_ROOT)}>Open the vault root</Link>
            </Button>
          }
          secondaryAction={
            <Button variant="quiet" size="sm" onClick={onRetry}>
              Refresh
            </Button>
          }
          className="min-h-[20rem]"
        />
      </Panel>
    )
  }

  const folders = childFolders(tree, folder)
  const notes = notesInFolder(tree, folder)
  const node = folder === VAULT_ROOT ? tree : findFolder(tree, folder)
  const total = node ? countNotesIn(node) : 0

  return (
    <Panel className="min-w-0 overflow-hidden">
      <PanelHeader
        eyebrow="Folder"
        title={folder === VAULT_ROOT ? 'Vault' : folderName(folder)}
        description={
          <span className="font-mono text-2xs text-ink-muted" data-selected-folder={folder}>
            {folder === VAULT_ROOT ? 'The vault root' : folder}
          </span>
        }
        actions={
          <div className="flex flex-wrap items-center gap-1.5">
            <Button size="sm" leadingIcon={<PlusIcon size={15} />} onClick={onNewNote} data-folder-action="new-note">
              New note
            </Button>
            <Button size="sm" variant="secondary" onClick={onNewFolder} data-folder-action="new-folder">
              New folder
            </Button>
            {folder === VAULT_ROOT ? null : (
              <Button
                size="sm"
                variant="secondary"
                onClick={onRenameOrMove}
                data-folder-action="rename"
              >
                Rename or move
              </Button>
            )}
          </div>
        }
      />
      <PanelBody className="min-w-0">
        {folders.length === 0 && notes.length === 0 ? (
          <EmptyState
            compact
            headingLevel="p"
            icon={<FolderIcon size={18} />}
            title="This folder is empty"
            description="Create a note here, or a folder to organise the next one."
            action={
              <Button variant="secondary" size="sm" onClick={onNewNote}>
                Create a note
              </Button>
            }
          />
        ) : (
          <div className="flex flex-col gap-5">
            {folders.length > 0 ? (
              <div>
                <p className="text-2xs font-semibold tracking-label text-ink-faint">
                  Folders
                </p>
                <ul className="mt-1.5 flex flex-col gap-1" aria-label="Folders">
                  {folders.map((child) => (
                    <li key={child.path}>
                      <Link
                        to={folderHref(child.path)}
                        data-folder-path={child.path}
                        className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-ink-soft transition-colors hover:bg-surface-hover/60 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                      >
                        <FolderIcon size={14} className="shrink-0 text-ink-muted" />
                        <span className="min-w-0 flex-1 truncate font-medium">{child.name}</span>
                        <span className="shrink-0 text-2xs tabular-nums text-ink-faint">
                          {noteCountLabel(countNotesIn(child))}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}

            <div>
              <p className="text-2xs font-semibold tracking-label text-ink-faint">Notes</p>
              {notes.length === 0 ? (
                <p className="mt-1.5 text-xs leading-5 text-ink-muted">
                  No note sits directly in this folder.
                </p>
              ) : (
                <ul className="mt-1.5 flex flex-col gap-1" aria-label="Notes in this folder">
                  {notes.map((note) => (
                    <li key={note.path}>
                      <Link
                        to={noteHref(note.path)}
                        data-note-path={note.path}
                        className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-ink-soft transition-colors hover:bg-surface-hover/60 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
                      >
                        <BookIcon size={14} className="shrink-0 text-ink-muted" />
                        <span className="min-w-0 flex-1 truncate font-medium">
                          {noteTitleFromPath(note.path)}
                        </span>
                        <span className="shrink-0 truncate font-mono text-2xs text-ink-faint">
                          {note.path}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              )}
            </div>

            <p className="text-2xs leading-4 text-ink-faint">
              {noteCountLabel(total)} under this folder, including its subfolders. Notes keep their
              stable ids when a folder moves, so links keep working.
            </p>
          </div>
        )}
      </PanelBody>
    </Panel>
  )
}
