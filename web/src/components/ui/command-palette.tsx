import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import { Dialog, DialogContent } from './dialog'
import { EmptyState } from './empty-state'
import { ErrorState } from './error-state'
import { BookIcon, FolderIcon, LayersIcon, SearchIcon } from './icons'
import { SkeletonText } from './skeleton'
import { cn } from './utils'

export interface Command {
  id: string
  label: string
  hint?: string
  icon?: ReactNode
  keywords?: string
  onSelect: () => void
}

export type PaletteSearchKind = 'project' | 'task' | 'note'

/** One server result, already adapted. The palette never invents a subtitle. */
export interface PaletteSearchResult {
  id: string
  kind: PaletteSearchKind
  title: string
  subtitle: string | null
  projectId: string | null
  path: string | null
}

export interface PaletteSearchGroup {
  kind: PaletteSearchKind
  label: string
  /** The server returned more than the shown rows. */
  truncated: boolean
  results: PaletteSearchResult[]
}

export interface PaletteSearchState {
  groups: PaletteSearchGroup[]
  /** Rows returned across all groups, not the server's match total. */
  total: number
  pending: boolean
  error: string | null
  onRetry: () => void
  onSelect: (result: PaletteSearchResult) => void
}

export interface CommandPaletteProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  commands?: Command[]
  /** True while the workspace has no commands registered at all. */
  emptyMessage?: string
  emptyHint?: string
  placeholder?: string
  /** Controlled input value, so the shell can reset it on close. */
  query: string
  onQueryChange: (query: string) => void
  /**
   * Server search. When the query is non-empty and this is present, commands
   * give way to the grouped results; with an empty query it is ignored.
   */
  search?: PaletteSearchState
}

function SearchResultIcon({ kind }: { kind: PaletteSearchKind }) {
  if (kind === 'project') return <FolderIcon size={15} />
  if (kind === 'task') return <LayersIcon size={15} />
  return <BookIcon size={15} />
}

/**
 * The workspace palette. Two modes share one input:
 *
 * - empty query: the commands the shell registered, filtered locally;
 * - non-empty query: grouped server results (Projects / Tasks / Knowledge).
 *
 * Results use one listbox with group labels and a single active option. Focus
 * stays in the input; `aria-activedescendant` names the option Enter would run,
 * so the arrow keys never move focus out of the field.
 */
export function CommandPalette({
  open,
  onOpenChange,
  commands = [],
  emptyMessage = 'No commands yet',
  emptyHint = 'Commands will appear here as workspace features connect.',
  placeholder = 'Search commands…',
  query,
  onQueryChange,
  search,
}: CommandPaletteProps) {
  const [activeIndex, setActiveIndex] = useState(0)
  const activeRef = useRef<HTMLButtonElement>(null)
  const listId = useId()

  const trimmedQuery = query.trim()
  const searchActive = trimmedQuery.length > 0 && search !== undefined
  const searchResults = useMemo(
    () => (searchActive ? (search?.groups ?? []).flatMap((group) => group.results) : []),
    [searchActive, search],
  )
  const filteredCommands = useMemo(() => {
    const needle = trimmedQuery.toLowerCase()
    if (!needle || searchActive) return commands
    return commands.filter((command) =>
      `${command.label} ${command.keywords ?? ''}`.toLowerCase().includes(needle),
    )
  }, [commands, trimmedQuery, searchActive])

  const optionCount = searchActive ? searchResults.length : filteredCommands.length
  const safeIndex = Math.min(activeIndex, Math.max(0, optionCount - 1))
  const hasOptions = optionCount > 0

  useEffect(() => {
    setActiveIndex(0)
  }, [query])

  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: 'nearest' })
  }, [safeIndex, searchActive])

  const choose = useCallback(
    (index: number) => {
      if (searchActive) {
        const result = searchResults[index]
        if (!result || !search) return
        onOpenChange(false)
        search.onSelect(result)
        return
      }
      const command = filteredCommands[index]
      if (!command) return
      onOpenChange(false)
      command.onSelect()
    },
    [filteredCommands, onOpenChange, search, searchActive, searchResults],
  )

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (optionCount === 0) return
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        setActiveIndex((index) => (index + 1) % optionCount)
      } else if (event.key === 'ArrowUp') {
        event.preventDefault()
        setActiveIndex((index) => (index - 1 + optionCount) % optionCount)
      } else if (event.key === 'Enter') {
        event.preventDefault()
        choose(safeIndex)
      }
    },
    [choose, optionCount, safeIndex],
  )

  const activeOptionId = hasOptions ? `${listId}-option-${safeIndex}` : undefined
  const announcement = searchActive
    ? search?.pending
      ? 'Searching…'
      : search?.error
        ? 'Search could not run.'
        : `${search?.total ?? 0} result${(search?.total ?? 0) === 1 ? '' : 's'} for ${trimmedQuery}.`
    : ''

  const renderSearchOption = (result: PaletteSearchResult, index: number) => (
    <button
      key={`${result.kind}-${result.id}`}
      id={`${listId}-option-${index}`}
      data-search-result={`${result.kind}:${result.id}`}
      ref={index === safeIndex ? activeRef : undefined}
      role="option"
      aria-selected={index === safeIndex}
      type="button"
      onMouseEnter={() => setActiveIndex(index)}
      onClick={() => choose(index)}
      className={cn(
        'flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm text-ink',
        index === safeIndex && 'bg-accent-soft text-accent-soft-ink',
      )}
    >
      <span className="shrink-0 text-ink-muted">
        <SearchResultIcon kind={result.kind} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium">{result.title}</span>
        {result.subtitle ? (
          <span className="block truncate text-2xs text-ink-muted">{result.subtitle}</span>
        ) : null}
      </span>
    </button>
  )

  let nextOptionIndex = 0

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Search or run a command"
        description="Search projects, tasks and knowledge, or run a workspace command."
        hideHeader
        showClose={false}
        className="max-w-xl overflow-hidden p-0"
      >
        <div data-command-palette="" className="flex min-w-0 flex-col">
          <div className="flex items-center gap-2.5 border-b border-line px-4">
            <SearchIcon size={17} className="shrink-0 text-ink-muted" />
            <input
              data-search-query=""
              value={query}
              maxLength={200}
              autoComplete="off"
              onChange={(event) => onQueryChange(event.target.value)}
              onKeyDown={handleKeyDown}
              role="combobox"
              aria-haspopup="listbox"
              aria-label={
                search
                  ? 'Search projects, tasks and knowledge'
                  : 'Search commands'
              }
              aria-expanded={hasOptions}
              aria-controls={hasOptions ? listId : undefined}
              aria-autocomplete="list"
              aria-activedescendant={activeOptionId}
              placeholder={placeholder}
              className="h-12 w-full bg-transparent text-base text-ink outline-none placeholder:text-ink-faint"
              autoFocus
            />
            <kbd className="command-key">Esc</kbd>
          </div>

          <p role="status" aria-live="polite" className="sr-only">
            {announcement}
          </p>

          <div
            data-search-state={
              searchActive ? (search?.pending ? 'pending' : search?.error ? 'error' : 'ready') : 'commands'
            }
            className="scrollbar-slim max-h-[min(60vh,24rem)] overflow-y-auto p-2"
          >
            {searchActive ? (
              <>
                {search?.pending && searchResults.length === 0 ? (
                  <div aria-busy="true" className="px-2 py-3">
                    <p className="mb-2 flex items-center gap-1.5 text-2xs text-ink-muted">
                      Searching projects, tasks and knowledge…
                    </p>
                    <SkeletonText lines={3} />
                  </div>
                ) : search?.error ? (
                  <ErrorState
                    compact
                    title="Search could not run"
                    description={search.error}
                    onRetry={search.onRetry}
                  />
                ) : searchResults.length === 0 ? (
                  <EmptyState
                    compact
                    headingLevel="p"
                    title={`No matches for “${trimmedQuery}”`}
                    description="Try a shorter word or a different name."
                    icon={<SearchIcon size={16} />}
                  />
                ) : (
                  <div id={listId} role="listbox" aria-label="Search results" className="flex flex-col gap-1">
                    {search?.groups.map((group) => {
                      if (group.results.length === 0) return null
                      const groupId = `${listId}-group-${group.kind}`
                      return (
                        <div
                          key={group.kind}
                          role="group"
                          aria-labelledby={groupId}
                          data-search-group={group.kind}
                          className="flex flex-col gap-0.5"
                        >
                          <p
                            id={groupId}
                            className="flex items-baseline justify-between gap-2 px-2.5 pb-0.5 pt-1.5 text-2xs font-semibold uppercase tracking-label text-ink-faint"
                          >
                            <span>{group.label}</span>
                            <span className="tabular-nums font-normal normal-case tracking-normal">
                              {group.results.length}
                              {group.truncated ? '+' : ''}
                            </span>
                          </p>
                          {group.results.map((result) => {
                            const index = nextOptionIndex
                            nextOptionIndex += 1
                            return renderSearchOption(result, index)
                          })}
                          {group.truncated ? (
                            <p className="px-2.5 pb-1 text-2xs leading-4 text-ink-faint">
                              More match than shown. Type more to narrow it.
                            </p>
                          ) : null}
                        </div>
                      )
                    })}
                  </div>
                )}
              </>
            ) : commands.length === 0 ? (
              <EmptyState
                compact
                headingLevel="p"
                title={emptyMessage}
                description={emptyHint}
                icon={<SearchIcon size={16} />}
              />
            ) : filteredCommands.length === 0 ? (
              <EmptyState
                compact
                headingLevel="p"
                title="No matching commands"
                description={`Nothing matches “${trimmedQuery}”. Try a different word.`}
                icon={<SearchIcon size={16} />}
              />
            ) : (
              <div id={listId} role="listbox" aria-label="Commands" className="flex flex-col gap-0.5">
                {filteredCommands.map((command, index) => (
                  <button
                    key={command.id}
                    id={`${listId}-option-${index}`}
                    ref={index === safeIndex ? activeRef : undefined}
                    role="option"
                    aria-selected={index === safeIndex}
                    type="button"
                    onMouseEnter={() => setActiveIndex(index)}
                    onClick={() => choose(index)}
                    className={cn(
                      'flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm text-ink',
                      index === safeIndex && 'bg-accent-soft text-accent-soft-ink',
                    )}
                  >
                    {command.icon ? <span className="shrink-0">{command.icon}</span> : null}
                    <span className="flex-1 truncate">{command.label}</span>
                    {command.hint ? (
                      <span className="text-xs text-ink-muted">{command.hint}</span>
                    ) : null}
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="flex items-center justify-between gap-3 border-t border-line bg-surface-sunken/60 px-4 py-2 text-2xs text-ink-muted">
            {searchActive ? (
              <span data-search-count="" className="truncate">
                {search?.pending
                  ? 'Searching…'
                  : search?.error
                    ? 'Search unavailable'
                    : `${search?.total ?? 0} result${(search?.total ?? 0) === 1 ? '' : 's'} for “${trimmedQuery}”`}
              </span>
            ) : (
              <span className="flex items-center gap-2">
                <kbd className="command-key">↑</kbd>
                <kbd className="command-key">↓</kbd>
                <span>to move</span>
                <kbd className="command-key">↵</kbd>
                <span>to run</span>
              </span>
            )}
            <span className="hidden shrink-0 sm:inline">Esc to close</span>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
