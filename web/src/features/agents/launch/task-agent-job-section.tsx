/**
 * The task drawer's launch surface.
 *
 * It derives whether a launch control can be offered from data the drawer
 * already has (the task status and this device's job overview) and then opens
 * the launch dialog. Eligibility here is an explanation, not a decision: the
 * server re-checks the status, the prerequisites, the one-active-job rule and
 * the executor enrolment, and its refusal is quoted in the dialog.
 */
import { useMemo, useState } from 'react'
import { Link } from 'react-router-dom'
import type { Task } from '@/api/adapters'
import { useAgentOverview } from '@/hooks'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Section } from '@/components/ui/section'
import { SkeletonText } from '@/components/ui/skeleton'
import { AgentsIcon, ArrowRightIcon } from '@/components/ui/icons'
import { agentStateLabel, agentStateTone, shortAgentId } from '../agent-model'
import { activeJobForTask, launchEligibility } from '../launch-model'
import { LaunchAgentJobDialog } from './launch-agent-job-dialog'
import { useExecutorState } from './use-executor-state'

export function TaskAgentJobSection({ task }: { task: Task }) {
  const overview = useAgentOverview()
  const executorState = useExecutorState()
  const [open, setOpen] = useState(false)

  const activeJob = useMemo(
    () => activeJobForTask(overview.data ?? [], task.id),
    [overview.data, task.id],
  )
  const eligibility = launchEligibility({ status: task.status, activeJob })
  const blocker = executorState.blocker
  const jobsPending = overview.isPending && !overview.data
  const canOpen = eligibility.eligible && !blocker && !jobsPending

  return (
    <Section
      title="Agent job"
      description="Queue this task for an enrolled executor. Queueing creates a job; the executor claims it before anything runs."
      aside={
        activeJob ? (
          <Badge tone={agentStateTone(activeJob.state)} dot>
            {agentStateLabel(activeJob.state)}
          </Badge>
        ) : null
      }
    >
      {jobsPending ? (
        <div aria-busy="true" aria-label="Checking this task's agent jobs">
          <SkeletonText lines={2} />
        </div>
      ) : activeJob ? (
        <div className="rounded-md border border-line bg-surface-sunken/40 px-3 py-2.5">
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
            <span className="text-sm text-ink">
              This task has an active job in {agentStateLabel(activeJob.state).toLowerCase()}.
            </span>
            <Badge tone="neutral">{activeJob.state}</Badge>
          </div>
          <p className="mt-1 text-xs leading-5 text-ink-muted">
            The server allows one active job per task, so no second job can be queued while this
            one exists. Read the job to see its claim state and handoffs.
          </p>
          <Link
            to={`/agents/${encodeURIComponent(activeJob.jobId)}`}
            data-agent-job={activeJob.jobId}
            className="mt-2 inline-flex items-center gap-1 rounded-sm text-xs font-medium text-accent-soft-ink underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
          >
            Open job {shortAgentId(activeJob.jobId)}
            <ArrowRightIcon size={12} />
          </Link>
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          <p className="text-xs leading-5 text-ink-muted">
            {eligibility.reason ??
              'This task can be queued for an executor. The job waits for a claim; this browser does not start a model.'}
          </p>
          {overview.error && !overview.data ? (
            <p className="text-2xs leading-4 text-warning-ink">
              This device&apos;s job list could not be read, so an active job may exist. The server
              still refuses a second job for the same task; its refusal is shown here.
            </p>
          ) : null}
          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              data-launch-agent-job=""
              onClick={() => setOpen(true)}
              disabled={!canOpen}
              leadingIcon={<AgentsIcon size={14} />}
              aria-describedby={blocker ? 'task-agent-launch-blocked' : undefined}
            >
              Queue an agent job
            </Button>
            {blocker ? (
              <p
                id="task-agent-launch-blocked"
                data-launch-blocked=""
                className="min-w-0 flex-1 text-2xs leading-4 text-ink-muted"
              >
                {blocker}
              </p>
            ) : null}
          </div>
        </div>
      )}

      {open ? (
        <LaunchAgentJobDialog fixedTask={task} onClose={() => setOpen(false)} />
      ) : null}
    </Section>
  )
}
