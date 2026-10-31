/**
 * The workflow-facing milestone create entry point.
 *
 * The rich dialog (description, priority, owner and dates) lives in
 * `features/milestones`; these wrappers keep the existing call sites and the
 * `NewMilestoneButton` header action working without a second form.
 */
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { PlusIcon } from '@/components/ui/icons'
import { MilestoneCreateDialog as RichMilestoneCreateDialog } from '@/features/milestones'

export function MilestoneCreateDialog({
  open,
  onOpenChange,
  projectId,
  projectName,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  projectId: string
  projectName?: string
}) {
  return (
    <RichMilestoneCreateDialog
      open={open}
      onOpenChange={onOpenChange}
      projectId={projectId}
      projectName={projectName}
    />
  )
}

/** The header action a milestone-bearing view offers. */
export function NewMilestoneButton({
  projectId,
  projectName,
  size = 'sm',
}: {
  projectId: string
  projectName?: string
  size?: 'sm' | 'md'
}) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button size={size} leadingIcon={<PlusIcon size={15} />} onClick={() => setOpen(true)}>
        New milestone
      </Button>
      <MilestoneCreateDialog
        open={open}
        onOpenChange={setOpen}
        projectId={projectId}
        projectName={projectName}
      />
    </>
  )
}
