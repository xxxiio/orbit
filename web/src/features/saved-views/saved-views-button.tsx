import { useMemo, useState } from 'react'
import { useLocation } from 'react-router-dom'
import type { SavedView } from '@/api/adapters'
import type { SavedViewDisplay } from '@/api/contracts'
import {
  classifyError,
  newCommandId,
  uncertainMessage,
  useDeleteSavedView,
  useIdempotentAttempt,
  useRenameSavedView,
  useSaveSavedView,
  useSavedViews,
  userMessage,
} from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent } from '@/components/ui/dialog'
import { EmptyState } from '@/components/ui/empty-state'
import { Field } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { SkeletonText } from '@/components/ui/skeleton'
import { useToast } from '@/components/ui/toast'
import { AlertCircleIcon, PinIcon, TrashIcon } from '@/components/ui/icons'
import { useSavedViewRestore, type SavedViewRestoreValue } from './context'
import {
  savedViewConfigFromLocation,
  savedViewConfigLabel,
  savedViewMatchesConfig,
  savedViewNameError,
} from './model'

type Mode = 'list' | 'save' | 'rename' | 'delete'

interface PinnedCommand {
  key: string
  commandId: string
}

interface PinnedSaveCommand extends PinnedCommand {
  viewId: string
}

function errorCopy(error: unknown, uncertain: boolean): string | null {
  if (!error) return null
  if (uncertain) return uncertainMessage(error)
  if (classifyError(error) === 'conflict') {
    return 'This saved view changed on another device. Close this prompt and choose the action again from the refreshed list.'
  }
  if (classifyError(error) === 'notFound') {
    return 'This saved view no longer exists. The list has been refreshed.'
  }
  return userMessage(error)
}

/**
 * Save / apply / rename / delete for personal saved views.
 *
 * The control only renders where the project workspace can restore a view.
 * Every action is one server command; a refusal is quoted in the same dialog,
 * and a stale version refreshes the list rather than retrying silently.
 */
export function SavedViewsButton({
  display,
}: {
  display: SavedViewDisplay
}) {
  const restore = useSavedViewRestore()
  if (!restore) return null
  return <SavedViewsDialog display={display} restore={restore} />
}

function SavedViewsDialog({
  display,
  restore,
}: {
  display: SavedViewDisplay
  restore: SavedViewRestoreValue
}) {
  const location = useLocation()
  const views = useSavedViews()
  const save = useSaveSavedView()
  const rename = useRenameSavedView()
  const remove = useDeleteSavedView()
  const { toast } = useToast()

  const saveAttempt = useIdempotentAttempt<PinnedSaveCommand>((key) => ({
    key,
    commandId: newCommandId(),
    viewId: `sv-${newCommandId()}`,
  }))
  const renameAttempt = useIdempotentAttempt<PinnedCommand>((key) => ({
    key,
    commandId: newCommandId(),
  }))
  const deleteAttempt = useIdempotentAttempt<PinnedCommand>((key) => ({
    key,
    commandId: newCommandId(),
  }))

  const config = useMemo(
    () => savedViewConfigFromLocation(location.pathname, location.search, display),
    [location.pathname, location.search, display],
  )

  const ownViews = useMemo(() => {
    if (!config) return []
    return (views.data ?? []).filter((view) => savedViewMatchesConfig(view, config))
  }, [views.data, config])

  const [open, setOpen] = useState(false)
  const [mode, setMode] = useState<Mode>('list')
  const [target, setTarget] = useState<SavedView | null>(null)
  const [name, setName] = useState('')
  const [nameTouched, setNameTouched] = useState(false)

  if (!config) return null

  const pending = save.isPending || rename.isPending || remove.isPending
  const nameError = nameTouched ? savedViewNameError(name) : null

  const resetActions = () => {
    save.reset()
    rename.reset()
    remove.reset()
    saveAttempt.clear()
    renameAttempt.clear()
    deleteAttempt.clear()
  }

  const backToList = () => {
    setMode('list')
    setTarget(null)
    setName('')
    setNameTouched(false)
    resetActions()
  }

  const openDialog = () => {
    backToList()
    setOpen(true)
  }

  const startSave = () => {
    setName('')
    setNameTouched(false)
    setTarget(null)
    save.reset()
    saveAttempt.clear()
    setMode('save')
  }

  const startRename = (view: SavedView) => {
    setName(view.name)
    setNameTouched(false)
    setTarget(view)
    rename.reset()
    renameAttempt.clear()
    setMode('rename')
  }

  const startDelete = (view: SavedView) => {
    setTarget(view)
    remove.reset()
    deleteAttempt.clear()
    setMode('delete')
  }

  const apply = (view: SavedView) => {
    restore.restore(view)
    setOpen(false)
    backToList()
  }

  const submitSave = async () => {
    setNameTouched(true)
    if (!config || savedViewNameError(name) !== null) return
    const trimmed = name.trim()
    const key = [config.scope, config.projectId ?? '', config.query, config.display, trimmed].join('|')
    const pinned = saveAttempt.ensure(key)
    try {
      await save.mutateAsync({
        commandId: pinned.commandId,
        viewId: pinned.viewId,
        name: trimmed,
        scope: config.scope,
        projectId: config.projectId,
        query: config.query,
        display: config.display,
      })
      toast({
        tone: 'success',
        title: 'Saved view created',
        description: `“${trimmed}” is stored on the server.`,
      })
      backToList()
    } catch {
      // `save.error` is shown in the dialog.
    }
  }

  const submitRename = async () => {
    setNameTouched(true)
    if (!target || savedViewNameError(name) !== null) return
    const trimmed = name.trim()
    const pinned = renameAttempt.ensure([target.id, target.version, trimmed].join('|'))
    try {
      await rename.mutateAsync({
        commandId: pinned.commandId,
        viewId: target.id,
        expectedVersion: target.version,
        name: trimmed,
      })
      toast({
        tone: 'success',
        title: 'Saved view renamed',
        description: `The view is now “${trimmed}”.`,
      })
      backToList()
    } catch {
      // `rename.error` is shown in the dialog.
    }
  }

  const submitDelete = async () => {
    if (!target) return
    const pinned = deleteAttempt.ensure([target.id, target.version].join('|'))
    try {
      await remove.mutateAsync({
        commandId: pinned.commandId,
        viewId: target.id,
        expectedVersion: target.version,
      })
      toast({
        tone: 'success',
        title: 'Saved view deleted',
        description: `“${target.name}” is no longer stored.`,
      })
      backToList()
    } catch {
      // `remove.error` is shown in the dialog.
    }
  }

  const actionError =
    mode === 'save'
      ? errorCopy(save.error, save.disposition === 'uncertain')
      : mode === 'rename'
        ? errorCopy(rename.error, rename.disposition === 'uncertain')
        : mode === 'delete'
          ? errorCopy(remove.error, remove.disposition === 'uncertain')
          : null

  const title =
    mode === 'save'
      ? 'Save current view'
      : mode === 'rename'
        ? 'Rename saved view'
        : mode === 'delete'
          ? 'Delete saved view'
          : 'Saved views'

  const description =
    mode === 'save'
      ? 'The current filters and display are stored under this name, on the server.'
      : mode === 'rename'
        ? 'Only the name changes. The stored filters and display stay as they are.'
        : mode === 'delete'
          ? 'The view is removed from the server for this account.'
          : 'Your own filters and display, stored on the server. They are personal and not part of the workflow.'

  return (
    <>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        leadingIcon={<PinIcon size={14} />}
        onClick={openDialog}
        aria-haspopup="dialog"
        data-saved-views-trigger=""
      >
        Saved views
        {ownViews.length > 0 ? (
          <span className="ml-1 rounded-full bg-surface-sunken px-1.5 text-2xs tabular-nums text-ink-muted">
            {ownViews.length}
          </span>
        ) : null}
      </Button>

      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!next) {
            setOpen(false)
            // Closing while a command is in flight is allowed: the answer is
            // still reported as a toast, and the dialog must never trap focus.
            if (!pending) backToList()
          } else {
            setOpen(true)
          }
        }}
      >
        <DialogContent title={title} description={description} size="md">
          <div className="flex flex-col gap-4" data-saved-views="" data-saved-view-mode={mode}>
            {mode === 'list' ? (
              <>
                <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2">
                  <p className="text-2xs font-semibold tracking-label text-ink-faint">
                    Current view
                  </p>
                  <p className="mt-0.5 break-all text-xs leading-5 text-ink-soft">
                    {savedViewConfigLabel(config)}
                  </p>
                </div>

                {views.isPending && !views.data ? (
                  <div aria-busy="true" data-saved-views-state="loading">
                    <SkeletonText lines={3} />
                  </div>
                ) : views.error ? (
                  <div
                    data-saved-views-state="error"
                    role="alert"
                    className="flex flex-wrap items-start gap-x-2 gap-y-1 rounded-md border border-warning/40 bg-warning-soft px-3 py-2 text-xs leading-5 text-warning-ink"
                  >
                    <AlertCircleIcon size={14} className="mt-0.5 shrink-0" />
                    <p className="min-w-0 flex-1">
                      {classifyError(views.error) === 'notFound'
                        ? 'Saved views are not available on this server build yet.'
                        : userMessage(views.error)}
                    </p>
                    <Button
                      type="button"
                      variant="quiet"
                      size="sm"
                      onClick={() => void views.refetch()}
                      loading={views.isFetching}
                    >
                      Retry
                    </Button>
                  </div>
                ) : ownViews.length === 0 ? (
                  <div data-saved-views-state="empty">
                    <EmptyState
                      compact
                      headingLevel="p"
                      icon={<PinIcon size={16} />}
                      title="No saved views for this project yet"
                      description="Save the filters you use often. The name, filters and display are stored on the server."
                    />
                  </div>
                ) : (
                  <ul
                    data-saved-views-state="ready"
                    aria-label="Saved views"
                    className="flex flex-col divide-y divide-line rounded-md border border-line"
                  >
                    {ownViews.map((view) => (
                      <li
                        key={view.id}
                        data-saved-view={view.id}
                        className="flex flex-col gap-2 px-3 py-2.5 sm:flex-row sm:items-center sm:justify-between"
                      >
                        <div className="min-w-0">
                          <p className="flex flex-wrap items-center gap-2 text-sm font-medium text-ink">
                            <span className="truncate">{view.name}</span>
                            <Badge tone="neutral">
                              {view.display === 'board' ? 'Board' : 'List'}
                            </Badge>
                            <span className="text-2xs font-normal tabular-nums text-ink-faint">
                              v{view.version}
                            </span>
                          </p>
                          <p className="mt-0.5 break-all font-mono text-2xs leading-4 text-ink-muted">
                            {view.query.length > 0 ? view.query : 'No extra filters'}
                          </p>
                        </div>
                        <div className="flex shrink-0 flex-wrap items-center gap-1">
                          <Button
                            type="button"
                            size="sm"
                            onClick={() => apply(view)}
                            data-saved-view-apply=""
                          >
                            Apply
                          </Button>
                          <Button
                            type="button"
                            variant="quiet"
                            size="sm"
                            onClick={() => startRename(view)}
                            data-saved-view-rename=""
                          >
                            Rename
                          </Button>
                          <Button
                            type="button"
                            variant="quiet"
                            size="sm"
                            onClick={() => startDelete(view)}
                            data-saved-view-delete=""
                            title={`Delete saved view ${view.name}`}
                            aria-label={`Delete saved view ${view.name}`}
                          >
                            <TrashIcon size={14} />
                          </Button>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}

                <div className="flex flex-wrap items-center justify-between gap-2 border-t border-line pt-3">
                  <p className="text-2xs leading-4 text-ink-muted">
                    Applying a view navigates to its saved query and list or board display.
                  </p>
                  <Button
                    type="button"
                    size="sm"
                    onClick={startSave}
                    disabled={pending}
                    data-saved-view-new=""
                  >
                    Save current view…
                  </Button>
                </div>
              </>
            ) : mode === 'delete' && target ? (
              <form
                className="flex flex-col gap-4"
                onSubmit={(event) => {
                  event.preventDefault()
                  void submitDelete()
                }}
              >
                <p className="text-sm leading-6 text-ink-soft">
                  Delete <span className="font-medium text-ink">“{target.name}”</span>? This removes
                  the saved filters and display from the server. It does not change any task.
                </p>
                {actionError ? (
                  <p role="alert" data-saved-view-error="" className="text-xs leading-5 text-danger-ink">
                    {actionError}
                  </p>
                ) : null}
                <div className="flex flex-wrap items-center justify-end gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={backToList}
                    disabled={remove.isPending}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="submit"
                    size="sm"
                    variant="secondary"
                    className="border-danger/40 text-danger-ink"
                    loading={remove.isPending}
                    data-saved-view-submit=""
                  >
                    Delete view
                  </Button>
                </div>
              </form>
            ) : (
              <form
                className="flex flex-col gap-4"
                onSubmit={(event) => {
                  event.preventDefault()
                  if (mode === 'save') void submitSave()
                  else void submitRename()
                }}
              >
                <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2">
                  <p className="text-2xs font-semibold tracking-label text-ink-faint">
                    {mode === 'save' ? 'Stores' : 'Stored configuration'}
                  </p>
                  <p className="mt-0.5 break-all text-xs leading-5 text-ink-soft">
                    {savedViewConfigLabel(
                      mode === 'rename' && target
                        ? { display: target.display, query: target.query }
                        : config,
                    )}
                  </p>
                </div>

                <Field
                  label="View name"
                  hint="A short name you will recognise in this project."
                  error={nameError ?? undefined}
                >
                  <Input
                    data-saved-view-name-input=""
                    value={name}
                    autoFocus
                    autoComplete="off"
                    disabled={save.isPending || rename.isPending}
                    onChange={(event) => setName(event.target.value)}
                    onBlur={() => setNameTouched(true)}
                  />
                </Field>

                {actionError ? (
                  <p role="alert" data-saved-view-error="" className="text-xs leading-5 text-danger-ink">
                    {actionError}
                  </p>
                ) : null}

                <div className="flex flex-wrap items-center justify-end gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={backToList}
                    disabled={save.isPending || rename.isPending}
                  >
                    Cancel
                  </Button>
                  <Button
                    type="submit"
                    size="sm"
                    loading={save.isPending || rename.isPending}
                    disabled={nameError !== null}
                    data-saved-view-submit=""
                  >
                    {mode === 'save' ? 'Save view' : 'Rename view'}
                  </Button>
                </div>
              </form>
            )}
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}
