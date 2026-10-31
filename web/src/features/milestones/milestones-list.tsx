import type { Milestone } from '@/api/adapters'
import { MilestoneCard } from './milestone-card'

/**
 * The milestone list: one card per checkpoint, in the server's order.
 *
 * The columns follow the *container*, not the viewport: this list is usually
 * rendered inside a workflow grid cell, which can be as narrow as a phone at any
 * window width. `auto-fit` keeps a card readable there instead of squeezing two
 * cards (and their actions) into a narrow panel.
 */
export function MilestonesList({
  milestones,
  onOpen,
  onEdit,
}: {
  milestones: Milestone[]
  onOpen: (milestone: Milestone) => void
  onEdit?: (milestone: Milestone) => void
}) {
  return (
    <ul className="grid gap-3 p-4 [grid-template-columns:repeat(auto-fit,minmax(15rem,1fr))]">
      {milestones.map((milestone) => (
        <MilestoneCard
          key={milestone.id}
          milestone={milestone}
          onOpen={onOpen}
          onEdit={onEdit}
        />
      ))}
    </ul>
  )
}
