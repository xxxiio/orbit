import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import { Dialog, DialogContent } from './dialog'
import { EmptyState } from './empty-state'
import { SearchIcon } from './icons'
import { cn } from './utils'

export interface Command {
  id: string
  label: string
  hint?: string
  icon?: ReactNode
  keywords?: string
  onSelect: () => void
}

export interface CommandPaletteProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  commands?: Command[]
  /** True while the workspace has no commands registered at all. */
  emptyMessage?: string
  emptyHint?: string
  placeholder?: string
}

/**
 * Deliberately small: a search field, a keyboard-navigable list, and honest
 * empty copy. Commands are supplied by whichever feature owns them; this lane
 * registers none, so the palette says so instead of inventing actions.
 */
export function CommandPalette({
  open,
  onOpenChange,
  commands = [],
  emptyMessage = 'No commands yet',
  emptyHint = 'Commands will appear here as workspace features connect.',
  placeholder = 'Search commands…',
}: CommandPaletteProps) {
  const [query, setQuery] = useState('')
  const [activeIndex, setActiveIndex] = useState(0)
  const activeRef = useRef<HTMLButtonElement>(null)
  const listId = useId()

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return commands
    return commands.filter((command) =>
      `${command.label} ${command.keywords ?? ''}`.toLowerCase().includes(needle),
    )
  }, [commands, query])

  const activeCommand = filtered[Math.min(activeIndex, Math.max(0, filtered.length - 1))]
  const hasResults = filtered.length > 0

  useEffect(() => {
    if (!open) {
      setQuery('')
      setActiveIndex(0)
    }
  }, [open])

  useEffect(() => {
    setActiveIndex(0)
  }, [query])

  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: 'nearest' })
  }, [activeIndex])

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (filtered.length === 0) return
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        setActiveIndex((index) => (index + 1) % filtered.length)
      } else if (event.key === 'ArrowUp') {
        event.preventDefault()
        setActiveIndex((index) => (index - 1 + filtered.length) % filtered.length)
      } else if (event.key === 'Enter') {
        event.preventDefault()
        const command = activeCommand
        if (command) {
          onOpenChange(false)
          command.onSelect()
        }
      }
    },
    [activeCommand, filtered.length, onOpenChange],
  )

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Command palette"
        description="Search for a command to run in this workspace."
        hideHeader
        showClose={false}
        className="max-w-xl overflow-hidden p-0"
      >
        <div className="flex items-center gap-2.5 border-b border-line px-4">
          <SearchIcon size={17} className="shrink-0 text-ink-muted" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={handleKeyDown}
            role="combobox"
            aria-expanded={hasResults}
            aria-controls={hasResults ? listId : undefined}
            aria-autocomplete="list"
            aria-activedescendant={
              hasResults && activeCommand ? `${listId}-option-${activeCommand.id}` : undefined
            }
            placeholder={placeholder}
            className="h-12 w-full bg-transparent text-base text-ink outline-none placeholder:text-ink-faint"
            autoFocus
          />
          <kbd className="command-key">Esc</kbd>
        </div>

        <div className="scrollbar-slim max-h-[min(60vh,22rem)] overflow-y-auto p-2">
          {commands.length === 0 ? (
            <EmptyState
              compact
              headingLevel="p"
              title={emptyMessage}
              description={emptyHint}
              icon={<SearchIcon size={16} />}
            />
          ) : filtered.length === 0 ? (
            <EmptyState
              compact
              headingLevel="p"
              title="No matching commands"
              description={`Nothing matches “${query.trim()}”. Try a different word.`}
              icon={<SearchIcon size={16} />}
            />
          ) : (
            <div id={listId} role="listbox" aria-label="Commands" className="flex flex-col gap-0.5">
              {filtered.map((command, index) => (
                <button
                  key={command.id}
                  id={`${listId}-option-${command.id}`}
                  ref={index === activeIndex ? activeRef : undefined}
                  role="option"
                  aria-selected={index === activeIndex}
                  type="button"
                  onMouseEnter={() => setActiveIndex(index)}
                  onClick={() => {
                    onOpenChange(false)
                    command.onSelect()
                  }}
                  className={cn(
                    'flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm text-ink',
                    index === activeIndex && 'bg-accent-soft text-accent-soft-ink',
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
          <span className="flex items-center gap-2">
            <kbd className="command-key">↑</kbd>
            <kbd className="command-key">↓</kbd>
            <span>to move</span>
            <kbd className="command-key">↵</kbd>
            <span>to run</span>
          </span>
          <span className="hidden sm:inline">Esc to close</span>
        </div>
      </DialogContent>
    </Dialog>
  )
}
