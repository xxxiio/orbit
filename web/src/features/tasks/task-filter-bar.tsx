import type { ReactNode, RefObject } from 'react'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { IconButton } from '@/components/ui/icon-button'
import { AlertCircleIcon, SearchIcon } from '@/components/ui/icons'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import {
  priorityFilterOptions,
  statusFilterOptions,
  type BulkAction,
  type SelectOption,
} from './task-list-model'
import { TASK_GROUP_OPTIONS, type TaskGroupKey } from './task-list-state'

export interface TaskFilterBarProps {
  search: string
  onSearchChange: (value: string) => void
  statusFilter: string
  onStatusFilterChange: (value: string) => void
  priorityFilter: string
  onPriorityFilterChange: (value: string) => void
  milestoneFilter?: string
  onMilestoneFilterChange?: (value: string) => void
  milestoneOptions?: SelectOption[]
  sortValue: string
  onSortValueChange: (value: string) => void
  sortOptions: SelectOption[]
  groupValue?: TaskGroupKey
  onGroupValueChange?: (value: TaskGroupKey) => void
  /** Extra controls the Tasks workspace keeps beside the filters. */
  trailing?: ReactNode
  searchPlaceholder: string
  searchLabel: string
  sortLabel: string
  searchRef?: RefObject<HTMLInputElement>
}

/**
 * Search, status, priority and sort for a task list. Both list pages render the
 * same controls; only their labels and sort vocabulary differ.
 */
export function TaskFilterBar({
  search,
  onSearchChange,
  statusFilter,
  onStatusFilterChange,
  priorityFilter,
  onPriorityFilterChange,
  milestoneFilter,
  onMilestoneFilterChange,
  milestoneOptions,
  sortValue,
  onSortValueChange,
  sortOptions,
  groupValue,
  onGroupValueChange,
  trailing,
  searchPlaceholder,
  searchLabel,
  sortLabel,
  searchRef,
}: TaskFilterBarProps) {
  return (
    <div className="flex flex-col gap-2.5 xl:flex-row xl:items-center">
      <div className="min-w-0 flex-1">
        <Input
          ref={searchRef}
          data-search-input=""
          type="search"
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          leadingIcon={<SearchIcon size={15} />}
          placeholder={searchPlaceholder}
          aria-label={searchLabel}
        />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Select value={statusFilter} onValueChange={onStatusFilterChange}>
          <SelectTrigger className="h-9 w-[9.5rem] text-sm" aria-label="Filter by status">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {statusFilterOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select value={priorityFilter} onValueChange={onPriorityFilterChange}>
          <SelectTrigger className="h-9 w-[9rem] text-sm" aria-label="Filter by priority">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {priorityFilterOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {milestoneFilter !== undefined && onMilestoneFilterChange && milestoneOptions ? (
          <Select value={milestoneFilter} onValueChange={onMilestoneFilterChange}>
            <SelectTrigger className="h-9 w-[11rem] text-sm" aria-label="Filter by milestone">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {milestoneOptions.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}

        <Select value={sortValue} onValueChange={onSortValueChange}>
          <SelectTrigger className="h-9 w-[10rem] text-sm" aria-label={sortLabel}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {sortOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                Sort: {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {groupValue !== undefined && onGroupValueChange ? (
          <Select
            value={groupValue}
            onValueChange={(value) => onGroupValueChange(value as TaskGroupKey)}
          >
            <SelectTrigger className="h-9 w-[11rem] text-sm" aria-label="Group tasks">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {TASK_GROUP_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  Group: {option.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
        {trailing}
      </div>
    </div>
  )
}

/** The inline reset shared by the filter summaries and empty states. */
export function ClearFiltersButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="ml-2 rounded-sm font-medium text-accent underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
    >
      Clear filters
    </button>
  )
}

export interface TaskBulkBarProps {
  selectedCount: number
  onClearSelection: () => void
  bulkAction: BulkAction
  onBulkActionChange: (action: BulkAction) => void
  actionOptions: Array<{ value: BulkAction; label: string }>
  bulkValue: string
  onBulkValueChange: (value: string) => void
  bulkOptions: SelectOption[]
  progress: { label: string; total: number; done: number } | null
  error: string | null
  onApply: () => void
}

/** The selection bar shared by Inbox and Project Tasks. */
export function TaskBulkBar({
  selectedCount,
  onClearSelection,
  bulkAction,
  onBulkActionChange,
  actionOptions,
  bulkValue,
  onBulkValueChange,
  bulkOptions,
  progress,
  error,
  onApply,
}: TaskBulkBarProps) {
  const busy = Boolean(progress)

  return (
    <section
      aria-label="Bulk actions"
      className="flex flex-wrap items-center gap-2 rounded-md border border-accent/30 bg-accent-soft/45 px-3 py-2"
    >
      <span className="flex items-center gap-2 text-xs font-medium text-accent-soft-ink">
        <Checkbox checked onCheckedChange={onClearSelection} aria-label="Clear selection" />
        {selectedCount} selected
      </span>

      <div className="ml-auto flex flex-wrap items-center gap-1.5">
        <Select
          value={bulkAction}
          onValueChange={(value) => onBulkActionChange(value as BulkAction)}
          disabled={busy}
        >
          <SelectTrigger className="h-8 w-[7.5rem] text-xs" aria-label="Bulk action">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {actionOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select value={bulkValue} onValueChange={onBulkValueChange} disabled={busy}>
          <SelectTrigger className="h-8 w-[11rem] text-xs" aria-label="Bulk value">
            <SelectValue placeholder="Choose a value" />
          </SelectTrigger>
          <SelectContent>
            {bulkOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Button size="sm" loading={busy} disabled={bulkValue.length === 0 || busy} onClick={onApply}>
          Apply
        </Button>
        <IconButton
          label="Clear selection"
          icon={<span aria-hidden="true">×</span>}
          size="sm"
          variant="quiet"
          onClick={onClearSelection}
          disabled={busy}
        />
      </div>

      {progress ? (
        <p className="w-full text-2xs text-accent-soft-ink" role="status" aria-live="polite">
          {progress.label}: {progress.done} of {progress.total} sent…
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="flex w-full items-start gap-1.5 text-2xs leading-4 text-danger-ink">
          <AlertCircleIcon size={12} className="mt-0.5 shrink-0" />
          {error}
        </p>
      ) : null}
    </section>
  )
}
