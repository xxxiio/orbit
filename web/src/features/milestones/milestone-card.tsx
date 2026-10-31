import type { Milestone } from '@/api/adapters'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { CalendarIcon } from '@/components/ui/icons'
import { milestoneStatusLabel } from '@/domain'
import {
  milestoneDateSummary,
  milestoneProgressLabel,
} from './milestone-model'

function statusTone(milestone: Milestone): 'success' | 'danger' | 'accent' | 'neutral' {
  if (milestone.status === 'completed') return 'success'
  if (milestone.status === 'blocked') return 'danger'
  if (milestone.status === 'in_progress') return 'accent'
  return 'neutral'
}

/**
 * A milestone card: the checkpoint, its status, its dates and how much of its
 * work is done. The whole card is not a button; the two actions are named so a
 * keyboard or screen-reader user never has to guess what a click will do.
 */
export function MilestoneCard({
  milestone,
  onOpen,
  onEdit,
}: {
  milestone: Milestone
  onOpen: (milestone: Milestone) => void
  onEdit?: (milestone: Milestone) => void
}) {
  const { blocked, percent } = milestone.progress

  return (
    <li
      data-milestone-card={milestone.id}
      className="group flex min-w-0 flex-col gap-3 rounded-lg border border-line bg-surface p-4 transition-[border-color,box-shadow] duration-150 ease-out hover:border-line-strong hover:shadow-sm"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-medium text-ink" title={milestone.name}>
            {milestone.name}
          </h3>
          <p className="mt-1 flex items-center gap-1 text-2xs text-ink-muted">
            <CalendarIcon size={12} className="shrink-0" />
            <span className="truncate">{milestoneDateSummary(milestone)}</span>
          </p>
        </div>
        <Badge tone={statusTone(milestone)} className="shrink-0 whitespace-nowrap">
          {milestoneStatusLabel(milestone.status)}
        </Badge>
      </div>

      <div>
        <div className="flex items-baseline justify-between gap-2 text-2xs">
          <span className="text-ink-muted">{milestoneProgressLabel(milestone)}</span>
          <span className="tabular-nums text-ink-faint">{percent}%</span>
        </div>
        <div
          role="progressbar"
          aria-label={`${milestone.name} progress`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
          className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-surface-sunken"
        >
          <span
            className="block h-full rounded-full bg-accent transition-[width] duration-300 ease-out"
            style={{ width: `${percent}%` }}
          />
        </div>
      </div>

      <div className="mt-auto flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-ink-muted">
        <span className="min-w-0 truncate">
          {milestone.owner ? `Owner ${milestone.owner}` : 'No owner'}
        </span>
        {blocked > 0 ? (
          <span className="inline-flex items-center gap-1 font-medium text-danger-ink">
            <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-danger" />
            {blocked} blocked
          </span>
        ) : null}
      </div>

      <div className="flex items-center gap-1.5">
        <Button
          size="sm"
          variant="secondary"
          onClick={() => onOpen(milestone)}
          aria-label={`Open details for ${milestone.name}`}
        >
          Open details
        </Button>
        {onEdit ? (
          <Button
            size="sm"
            variant="quiet"
            onClick={() => onEdit(milestone)}
            aria-label={`Edit ${milestone.name}`}
          >
            Edit
          </Button>
        ) : null}
      </div>
    </li>
  )
}
