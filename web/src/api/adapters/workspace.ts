/**
 * The workspace snapshot and the evidence-shaped records inside it.
 *
 * The snapshot is one server projection: schedules, strategies, routines, habits
 * and metrics are mapped here because they arrive with the tasks and projects,
 * not because the UI computes anything from them.
 */
import type {
  HabitRecord,
  MetricRecord,
  MilestoneRecord,
  ProjectRecord,
  RoutineRecord,
  ScheduleRecord,
  StrategyRecord,
  TaskDependencyEdge,
  TaskRecord,
} from '../contracts'
import { toMilestone, toProject, type Milestone, type Project } from './projects'
import { toDependency, toTask, type Dependency, type Task } from './tasks'

export interface ScheduleBlock {
  id: string
  title: string
  startsAtMs: number
  endsAtMs: number
  durationMinutes: number
  flexibility: string
  targetKind: string
  targetId: string | null
}

export function toScheduleBlock(record: ScheduleRecord): ScheduleBlock {
  return {
    id: record.id,
    title: record.title,
    startsAtMs: record.starts_at_ms,
    endsAtMs: record.ends_at_ms,
    durationMinutes: record.duration_minutes,
    flexibility: record.flexibility,
    targetKind: record.target_kind,
    targetId: record.target_id,
  }
}

export interface StrategyCriterion {
  key: string
  description: string
  required: boolean
  status: string
}

export interface Strategy {
  projectId: string
  mode: string
  objective: string | null
  progressSignals: string[]
  criteria: StrategyCriterion[]
}

export function toStrategy(record: StrategyRecord): Strategy {
  return {
    projectId: record.project_id,
    mode: record.mode,
    objective: record.objective,
    progressSignals: record.progress_signals,
    criteria: record.criteria.map((criterion) => ({ ...criterion })),
  }
}

export interface Routine {
  id: string
  name: string
  habitIds: string[]
}

export function toRoutine(record: RoutineRecord): Routine {
  return { id: record.id, name: record.name, habitIds: record.habit_ids }
}

export interface Habit {
  id: string
  name: string
  cadence: string
}

export function toHabit(record: HabitRecord): Habit {
  return { id: record.id, name: record.name, cadence: record.cadence }
}

export interface Metric {
  id: string
  baseline: number
  target: number
  direction: string
}

export function toMetric(record: MetricRecord): Metric {
  return { id: record.id, baseline: record.baseline, target: record.target, direction: record.direction }
}

/* ------------------------------------------------------------------ */
/* Workspace snapshot                                                  */
/* ------------------------------------------------------------------ */

export interface WorkspaceSnapshot {
  revision: number
  historyFingerprint: string
  projects: Project[]
  tasks: Task[]
  taskDependencies: Dependency[]
  milestones: Milestone[]
  strategies: Strategy[]
  schedule: ScheduleBlock[]
  routines: Routine[]
  habits: Habit[]
  metrics: Metric[]
}

export function toWorkspaceSnapshot(snapshot: {
  revision: number
  history_fingerprint: string
  projects: ProjectRecord[]
  tasks: TaskRecord[]
  task_dependencies: TaskDependencyEdge[]
  milestones: MilestoneRecord[]
  strategies: StrategyRecord[]
  schedule: ScheduleRecord[]
  routines: RoutineRecord[]
  habits: HabitRecord[]
  metrics: MetricRecord[]
}): WorkspaceSnapshot {
  return {
    revision: snapshot.revision,
    historyFingerprint: snapshot.history_fingerprint,
    projects: snapshot.projects.map(toProject),
    tasks: snapshot.tasks.map(toTask),
    taskDependencies: snapshot.task_dependencies.map(toDependency),
    milestones: snapshot.milestones.map(toMilestone),
    strategies: snapshot.strategies.map(toStrategy),
    schedule: snapshot.schedule.map(toScheduleBlock),
    routines: snapshot.routines.map(toRoutine),
    habits: snapshot.habits.map(toHabit),
    metrics: snapshot.metrics.map(toMetric),
  }
}
