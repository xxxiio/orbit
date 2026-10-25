/**
 * Deterministic workspace fixture, seeded through the *public* authoritative
 * HTTP API — never SQL.
 *
 * Two consumers:
 *  - `web/tests/e2e/**` (Playwright) seeds an isolated temp database per test;
 *  - later dogfooding / manual QA seeds a long-lived local server with
 *    `seedWorkspaceForServer({ base, deviceId, token })`.
 *
 * Guarantees the acceptance harness depends on:
 *  - fixed ids (no `Math.random`, no `crypto.randomUUID`) so assertions and
 *    screenshots are comparable between runs;
 *  - replayable command ids: on a fresh database every command is accepted
 *    exactly once, and a second run of the same seeder over the same database
 *    is a no-op replay (`duplicate: true`) rather than a conflict;
 *  - every mutation goes through the same endpoints the web app uses, so the
 *    seeded state is authoritative rather than a shortcut around the server;
 *  - loud failures: any unexpected status throws with endpoint, status and the
 *    response body.
 *
 * Shapes come from `.slim/deepwork/m31-web-app/api-contract.md`.
 */

export type TaskStatus =
  | 'inbox'
  | 'planned'
  | 'ready'
  | 'in_progress'
  | 'waiting'
  | 'blocked'
  | 'review'
  | 'deferred'
  | 'done'
  | 'cancelled'

export type TaskPriority = 'low' | 'medium' | 'high' | 'urgent'

export const TASK_STATUSES: readonly TaskStatus[] = [
  'inbox',
  'planned',
  'ready',
  'in_progress',
  'waiting',
  'blocked',
  'review',
  'deferred',
  'done',
  'cancelled',
]

/** Valid `inbox → …` command paths for each terminal/intermediate status. */
const STATUS_PATH: Record<TaskStatus, readonly TaskStatus[]> = {
  inbox: [],
  planned: ['planned'],
  ready: ['ready'],
  in_progress: ['ready', 'in_progress'],
  waiting: ['ready', 'in_progress', 'waiting'],
  blocked: ['ready', 'blocked'],
  review: ['ready', 'in_progress', 'review'],
  deferred: ['deferred'],
  done: ['ready', 'in_progress', 'done'],
  cancelled: ['cancelled'],
}

const HOUR = 3_600_000
const DAY = 24 * HOUR

export interface SeedProject {
  id: string
  name: string
  /** Bundled workflow id pinned at creation. */
  workflow: string
}

/** Seven projects: the five bundled workflows, the dogfooding project, and a guarded demo. */
export const SEED_PROJECTS: readonly SeedProject[] = [
  { id: 'p-orbit-development', name: 'Orbit Development', workflow: 'development' },
  { id: 'p-basic', name: 'Basic', workflow: 'basic' },
  { id: 'p-development', name: 'Development', workflow: 'development' },
  { id: 'p-research', name: 'Research', workflow: 'research' },
  { id: 'p-learning', name: 'Learning', workflow: 'learning' },
  { id: 'p-habits', name: 'Habits', workflow: 'habits' },
  { id: 'p-guarded', name: 'Guarded release', workflow: 'guarded' },
]

export const PROJECT_IDS = {
  orbit: 'p-orbit-development',
  basic: 'p-basic',
  development: 'p-development',
  research: 'p-research',
  learning: 'p-learning',
  habits: 'p-habits',
  guarded: 'p-guarded',
} as const

interface ProjectPlan {
  id: string
  verb: string
  nouns: readonly string[]
  labels: readonly string[]
}

const PROJECT_PLANS: readonly ProjectPlan[] = [
  {
    id: PROJECT_IDS.orbit,
    verb: 'Implement',
    labels: ['web', 'api', 'ui', 'harness', 'docs'],
    nouns: [
      'the session cookie flow',
      'the web session CSRF guard',
      'SPA static fallback routing',
      'the task detail drawer',
      'inbox quick capture',
      'the project creation dialog',
      'workflow view tabs',
      'board drag interactions',
      'planning handoff preview',
      'attachment import',
      'the markdown editor preview',
      'acceptance criteria reorder',
      'the dependency inspect panel',
      'readiness explanations',
      'next action ranking',
      'the workload insights panel',
      'the day plan builder',
      'schedule conflict hints',
      'the milestone timeline',
      'the agent attention queue',
      'the handoff review flow',
      'the resume context view',
      'knowledge backlinks',
      'the metric history chart',
      'the habit tracker grid',
      'the routine editor',
      'the strategy objective editor',
      'evidence binding',
      'the upgrade preview dialog',
      'the project switcher',
    ],
  },
  {
    id: PROJECT_IDS.development,
    verb: 'Add',
    labels: ['server', 'storage', 'tests', 'runtime'],
    nouns: [
      'command replay tests',
      'revision conflict tests',
      'attachment checksum validation',
      'markdown manifest digests',
      'the device rotation endpoint',
      'CSRF middleware tests',
      'session expiry handling',
      'snapshot projection guards',
      'readiness edge cases',
      'dependency cycle detection',
      'the schedule overlap query',
      'the milestone status machine',
      'workflow upgrade fingerprints',
      'bundled workflow manifests',
      'template planning',
      'folder move reconciliation',
      'sqlite busy handling',
      'the WAL checkpoint policy',
      'backup and restore checks',
      'proposal leasing',
      'the automation scheduler',
      'MCP tool surface guards',
      'console security headers',
      'error envelope consistency',
    ],
  },
  {
    id: PROJECT_IDS.research,
    verb: 'Investigate',
    labels: ['research', 'evidence', 'notes'],
    nouns: [
      'local-first conflict resolution',
      'workflow version upgrade safety',
      'readiness heuristics',
      'planning under uncertainty',
      'session replay attacks',
      'attachment deduplication',
      'offline snapshot verification',
      'knowledge graph backlinks',
      'metric observation cadence',
      'habit streak semantics',
      'agent handoff review policy',
      'revision fingerprinting',
      'schedule elasticity',
      'project handoff round trips',
      'capacity planning error bars',
      'dependency bottleneck detection',
      'markdown identity collisions',
      'device revocation effects',
      'next action explainability',
      'template instantiation edge cases',
      'workload estimation drift',
      'evidence retention rules',
    ],
  },
  {
    id: PROJECT_IDS.learning,
    verb: 'Study',
    labels: ['learning', 'reading', 'practice'],
    nouns: [
      'axum 0.8 extractor ordering',
      'SQLite WAL durability',
      'React Server Component trade-offs',
      'idempotency keys in practice',
      'event sourcing reconstruction',
      'CRDT convergence basics',
      'Playwright auto-waiting semantics',
      'ARIA dialog patterns',
      'Tailwind token architecture',
      'TanStack Query invalidation',
      'Herdr agent runtime lifecycle',
      'Markdown frontmatter parsing',
      'optimistic UI pitfalls',
      'keyboard navigation grids',
      'time zone safe planning',
      'integer money arithmetic',
      'capability based authorization',
      'workflow state machines',
      'observability for local daemons',
      'backup verification drills',
    ],
  },
  {
    id: PROJECT_IDS.habits,
    verb: 'Practice',
    labels: ['habit', 'health', 'focus'],
    nouns: [
      'a 25 minute focus block',
      'the morning review',
      'a daily walk',
      'inbox zero triage',
      'a weekly retro',
      'incremental note taking',
      'a two minute stretch break',
      'reading before bed',
      'a no-notification hour',
      'the evening shutdown ritual',
      'a deep work session',
      'hydration reminders',
      'a daily plan review',
      'a gratitude note',
    ],
  },
  {
    id: PROJECT_IDS.basic,
    verb: 'Tidy',
    labels: ['chores', 'admin'],
    nouns: [
      'the downloads folder',
      'expense receipts',
      'the password rotation list',
      'meeting notes archive',
      'the backups checklist',
      'invoices for the quarter',
      'the shared drive permissions',
      'old branch hygiene',
      'the printer queue',
      'calendar duplicates',
      'the bookmarks bar',
      'loose desktop files',
    ],
  },
]

const PRIORITY_CYCLE: readonly (TaskPriority | null)[] = [
  'high',
  'medium',
  'urgent',
  'low',
  'medium',
  'high',
  null,
  'medium',
]

const DESCRIPTION_TEMPLATES = [
  'Came up while reviewing the readiness report. Keep the change small and explain the trade-off in the pull request.',
  'Blocks the acceptance walkthrough. Needs a failing test first, then the smallest change that makes it pass.',
  'Follow-up from the last planning session. Confirm the server contract before touching the UI.',
  '',
  'Split from a larger item so it can be scheduled inside one focus block. Update the notes when done.',
  'Watch for a stale `expected_revision` here — the drawer keeps its own copy until the server answers.',
]

export interface SeedMilestonePlan {
  id: string
  projectId: string
  name: string
}

const MILESTONES: readonly SeedMilestonePlan[] = [
  { id: 'm-orbit-i1', projectId: PROJECT_IDS.orbit, name: 'I1 acceptance' },
  { id: 'm-orbit-i2', projectId: PROJECT_IDS.orbit, name: 'I2 workspaces' },
  { id: 'm-dev-reliability', projectId: PROJECT_IDS.development, name: 'Reliability' },
  { id: 'm-research-q1', projectId: PROJECT_IDS.research, name: 'Q1 findings' },
  { id: 'm-learning-basics', projectId: PROJECT_IDS.learning, name: 'Fundamentals' },
  { id: 'm-habits-daily', projectId: PROJECT_IDS.habits, name: 'Daily baseline' },
  { id: 'm-basic-cleanup', projectId: PROJECT_IDS.basic, name: 'Housekeeping' },
]

/**
 * A small, explicitly published workflow used by the work-item acceptance test.
 * It declares a guarded transition, so the browser can exercise a *server*
 * refusal from a declared move (the three bundled workflows only declare
 * unguarded moves). Published before the projects so `p-guarded` can bind it.
 */
const GUARDED_WORKFLOW = {
  id: 'guarded',
  version: 1,
  name: 'Guarded release',
  description: 'Tickets whose approval requires a high priority.',
  required_capabilities: [],
  work_items: [
    {
      id: 'ticket',
      name: 'Ticket',
      initial_state: 'open',
      states: ['open', 'approved', 'rejected'],
      fields: [
        {
          id: 'priority',
          kind: { type: 'choice', options: ['low', 'high'] },
          required: true,
          default: 'low',
        },
        { id: 'note', kind: { type: 'text' }, required: false, default: null },
      ],
      transitions: [
        {
          from: 'open',
          to: 'approved',
          guards: [
            { type: 'property_equals', field: 'priority', value: 'high' },
          ],
        },
        {
          from: 'open',
          to: 'rejected',
          guards: [{ type: 'property_present', field: 'note' }],
        },
      ],
    },
  ],
  views: [
    {
      id: 'overview',
      title: 'Overview',
      description: 'Tickets and their guarded moves.',
      layout: { type: 'vertical' },
      components: [
        {
          id: 'board',
          type: 'kanban',
          source: 'work_items',
          group_by: 'state',
          limit: 100,
          title: 'Tickets',
        },
        {
          id: 'items',
          type: 'table',
          source: 'work_items',
          columns: ['title', 'kind', 'state'],
          limit: 100,
          title: 'Ticket table',
        },
        {
          id: 'descending_tasks',
          type: 'list',
          source: 'tasks',
          sort: 'title',
          descending: true,
          limit: 5,
          title: 'Titles descending',
        },
      ],
    },
  ],
}

export interface SeedWorkItem {
  id: string
  projectId: string
  kind: string
  state: string
  properties: Record<string, unknown>
}

interface SeedWorkItemPlan extends SeedWorkItem {
  /** States to move through after creation, in order. */
  transitions: string[]
}

/**
 * Work items for the projects whose surfaces are exercised by I3. The
 * `p-development` project deliberately has none: the I2 settings test re-pins
 * it to Basic, and live items of a removed kind are a server-side blocker.
 */
export const WORK_ITEM_PLANS: readonly SeedWorkItemPlan[] = [
  {
    id: 'wi-orbit-feature-backlog',
    projectId: PROJECT_IDS.orbit,
    kind: 'feature',
    state: 'backlog',
    properties: { priority: 'medium' },
    transitions: [],
  },
  {
    id: 'wi-orbit-feature-ready',
    projectId: PROJECT_IDS.orbit,
    kind: 'feature',
    state: 'ready',
    properties: { priority: 'high' },
    transitions: ['ready'],
  },
  {
    id: 'wi-orbit-feature-review',
    projectId: PROJECT_IDS.orbit,
    kind: 'feature',
    state: 'review',
    properties: { priority: 'low' },
    transitions: ['ready', 'implementing', 'review'],
  },
  {
    id: 'wi-research-hypothesis-proposed',
    projectId: PROJECT_IDS.research,
    kind: 'hypothesis',
    state: 'proposed',
    properties: { question: 'Does the upgrade preview fence stale clients?', confidence: 'medium' },
    transitions: [],
  },
  {
    id: 'wi-research-hypothesis-investigating',
    projectId: PROJECT_IDS.research,
    kind: 'hypothesis',
    state: 'investigating',
    properties: { question: 'Which readiness signals predict blocked work?', confidence: 'low' },
    transitions: ['investigating'],
  },
  {
    id: 'wi-research-experiment-designed',
    projectId: PROJECT_IDS.research,
    kind: 'experiment',
    state: 'designed',
    properties: { method: 'Replay the same command from two sessions.', reproducible: true },
    transitions: [],
  },
  {
    id: 'wi-learning-lesson-planned',
    projectId: PROJECT_IDS.learning,
    kind: 'lesson',
    state: 'planned',
    properties: { topic: 'Typed workflow declarations', minutes: 45, difficulty: 'beginner' },
    transitions: [],
  },
  {
    id: 'wi-learning-lesson-studying',
    projectId: PROJECT_IDS.learning,
    kind: 'lesson',
    state: 'studying',
    properties: { topic: 'Revision fences', minutes: 30, difficulty: 'intermediate' },
    transitions: ['studying'],
  },
  {
    id: 'wi-learning-assessment-planned',
    projectId: PROJECT_IDS.learning,
    kind: 'assessment',
    state: 'planned',
    properties: { subject: 'Workflow upgrades', passed: false },
    transitions: [],
  },
  {
    id: 'wi-habits-practice-planned',
    projectId: PROJECT_IDS.habits,
    kind: 'practice',
    state: 'planned',
    properties: { activity: 'Morning walk', weekly_target: 3, check_in_required: true },
    transitions: [],
  },
  {
    id: 'wi-habits-practice-active',
    projectId: PROJECT_IDS.habits,
    kind: 'practice',
    state: 'active',
    properties: { activity: 'Evening reading', weekly_target: 5, check_in_required: false },
    transitions: ['active'],
  },
  {
    id: 'wi-habits-check-in-due',
    projectId: PROJECT_IDS.habits,
    kind: 'check_in',
    state: 'due',
    properties: { notes: 'Record the morning walk', performed: false },
    transitions: [],
  },
  {
    id: 'wi-guarded-ticket-open',
    projectId: PROJECT_IDS.guarded,
    kind: 'ticket',
    state: 'open',
    properties: { priority: 'high', note: 'Guard copy reviewed in the fixture.' },
    transitions: [],
  },
]

export interface SeedTask {
  id: string
  title: string
  status: TaskStatus
  projectId: string
  milestoneId: string | null
  priority: TaskPriority | null
  labels: string[]
  estimatedMinutes: number | null
  deadlineAtMs: number | null
  overdue: boolean
  dueToday: boolean
}

export interface SeedNote {
  id: string
  path: string
  title: string
}

export interface SeedAgentJob {
  id: string
  taskId: string
  handoffId: string | null
}

export interface WorkspaceFixture {
  revision: number
  nowMs: number
  projects: SeedProject[]
  tasks: SeedTask[]
  workItems: SeedWorkItem[]
  milestones: SeedMilestonePlan[]
  notes: SeedNote[]
  agentJobs: SeedAgentJob[]
  ids: {
    /** Task ids grouped for specs that assert on a single status bucket. */
    b: {
      overdue: string[]
      dueToday: string[]
      ready: string[]
      blocked: string[]
      inProgress: string[]
      inbox: string[]
      done: string[]
    }
    /** A task in `p-orbit-development` used by the task/conflict specs. */
    orbitTask: string
    /**
     * The row each Home section renders first: sections sort by earliest
     * deadline (no-deadline rows last), and only the first few rows are shown
     * before "Show all" — which is currently capped by a UI defect.
     */
    firstOverdue: string | null
    firstDueToday: string | null
  }
  counts: {
    total: number
    workItems: number
    byStatus: Record<TaskStatus, number>
    byProject: Record<string, number>
    overdue: number
    dueToday: number
    ready: number
    blocked: number
  }
}

/** Minimal transport the seeder needs. Implemented for bearer and cookie auth. */
export interface SeedTransport {
  post<T = unknown>(path: string, body: unknown): Promise<T>
  /** Authenticated GET, used to skip work a previous seed already did. */
  get<T = unknown>(path: string): Promise<T>
  /** Raw-bytes POST (attachment import) with query metadata in `path`. */
  postBytes<T = unknown>(path: string, bytes: Uint8Array, contentType: string): Promise<T>
  /**
   * Advisory read of the current authoritative revision, for diagnostics.
   * Seeding derives `expected_revision` from its own command sequence instead,
   * which is what makes a replay (same clock) a no-op.
   */
  revision(): Promise<number>
}

export class SeedError extends Error {
  constructor(
    readonly path: string,
    readonly status: number,
    readonly body: string,
    label: string,
  ) {
    super(`seed fixture: ${label} failed: POST ${path} → ${status} ${body}`)
    this.name = 'SeedError'
  }
}

export interface SeedOptions {
  /**
   * Injectable clock for the deadlines.
   *
   * Deadlines are relative to it, and they are part of the command payload, so a
   * replay of an already-seeded database must use the *same* value. The e2e
   * harness leaves it at `Date.now()` for a fresh database per test; pass a
   * fixed timestamp when re-seeding the same database on purpose.
   */
  nowMs?: number
  /** Identity recorded on seeded agent jobs. Defaults to `orbit-e2e-machine`. */
  machineRef?: string
  /** Skip the markdown/attachment/agent-job extras for fast UI-only tests. */
  extras?: boolean
}

/**
 * Runs the whole fixture. Resolves with the facts specs assert against, so no
 * test has to re-derive state or guess at counts.
 */
export async function seedWorkspace(
  transport: SeedTransport,
  options: SeedOptions = {},
): Promise<WorkspaceFixture> {
  const nowMs = options.nowMs ?? Date.now()
  const machineRef = options.machineRef ?? 'orbit-e2e-machine'
  const extras = options.extras !== false
  /**
   * The fixture is a fixed command sequence, so the expected revision is the
   * command index: 0 on a fresh database, and identical on a replay (the server
   * matches a duplicate by command id *and* payload, which includes this field).
   * That is what makes a second run a no-op instead of a conflict. Anything else
   * writing to the database first is a loud failure, not a silent divergence.
   */
  let revision = 0
  const advance = (label: string, next: number) => {
    if (next !== revision + 1) {
      throw new Error(
        `seed fixture: ${label} advanced the authority revision to ${next}, expected ${revision + 1}. ` +
          'The database is not the one this fixture was built against.',
      )
    }
    revision = next
  }

  const command = async <T extends { revision: number }>(
    path: string,
    body: Record<string, unknown>,
    label: string,
  ): Promise<T> => {
    try {
      const result = await transport.post<T>(path, { ...body, expected_revision: revision })
      advance(label, result.revision)
      return result
    } catch (error) {
      throw withLabel(error, label, path)
    }
  }

  // 1. A published workflow used by the work-item acceptance test, then the
  //    projects (creation is the only project write; it also pins a workflow).
  await transport
    .post('/v1/authoritative/blueprints', { definition: GUARDED_WORKFLOW })
    .catch((error) => {
      throw withLabel(error, 'guarded workflow', '/v1/authoritative/blueprints')
    })
  for (const project of SEED_PROJECTS) {
    await command(
      '/v1/authoritative/project-commands',
      {
        command_id: `seed-project-${project.id}`,
        project_id: project.id,
        name: project.name,
        workflow_id: project.workflow,
      },
      `project ${project.id}`,
    )
  }

  // 1b. Work items for the surfaces I3 renders. These are outside the domain
  //     ledger (they carry their own `u64` revision), so they do not advance
  //     the command sequence below. A replay skips any item that already
  //     exists instead of re-creating it.
  const existingItems = new Map<string, Set<string>>()
  const workItems: SeedWorkItem[] = []
  for (const plan of WORK_ITEM_PLANS) {
    let known = existingItems.get(plan.projectId)
    if (!known) {
      const listed = await transport
        .get<Array<{ id: string }>>(
          `/v1/authoritative/blueprints/items?project_id=${encodeURIComponent(plan.projectId)}`,
        )
        .catch((error) => {
          throw withLabel(error, `work items ${plan.projectId}`, '/v1/authoritative/blueprints/items')
        })
      known = new Set(listed.map((item) => item.id))
      existingItems.set(plan.projectId, known)
    }
    if (!known.has(plan.id)) {
      const created = await transport
        .post<{ id: string; state: string; revision: number }>(
          '/v1/authoritative/blueprints/items',
          {
            id: plan.id,
            project_id: plan.projectId,
            kind: plan.kind,
            properties: plan.properties,
          },
        )
        .catch((error) => {
          throw withLabel(error, `work item ${plan.id}`, '/v1/authoritative/blueprints/items')
        })
      let revision = created.revision
      let state = created.state
      for (const to of plan.transitions) {
        const moved = await transport
          .post<{ state: string; revision: number }>('/v1/authoritative/blueprints/transition', {
            id: plan.id,
            to,
            expected_revision: revision,
          })
          .catch((error) => {
            throw withLabel(error, `work item ${plan.id} -> ${to}`, '/v1/authoritative/blueprints/transition')
          })
        revision = moved.revision
        state = moved.state
      }
      if (state !== plan.state) {
        throw new Error(
          `seed fixture: work item ${plan.id} ended in ${state}, expected ${plan.state}`,
        )
      }
      known.add(plan.id)
    }
    workItems.push({
      id: plan.id,
      projectId: plan.projectId,
      kind: plan.kind,
      state: plan.state,
      properties: plan.properties,
    })
  }

  // 2. Milestones.
  for (const milestone of MILESTONES) {
    await command(
      '/v1/authoritative/milestone-commands',
      {
        command_id: `seed-milestone-${milestone.id}`,
        action: {
          type: 'create',
          milestone_id: milestone.id,
          project_id: milestone.projectId,
          name: milestone.name,
        },
      },
      `milestone ${milestone.id}`,
    )
  }
  // A started and a finished checkpoint so timelines have more than one colour.
  for (const [milestoneId, status] of [
    ['m-orbit-i1', 'in_progress'],
    ['m-dev-reliability', 'in_progress'],
    ['m-habits-daily', 'completed'],
  ] as const) {
    if (status === 'in_progress') {
      await command(
        '/v1/authoritative/milestone-commands',
        {
          command_id: `seed-milestone-${milestoneId}-start`,
          action: { type: 'change_status', milestone_id: milestoneId, status },
        },
        `milestone ${milestoneId} status`,
      )
    } else {
      // planned → in_progress → completed is the only legal path.
      for (const step of ['in_progress', 'completed'] as const) {
        await command(
          '/v1/authoritative/milestone-commands',
          {
            command_id: `seed-milestone-${milestoneId}-${step}`,
            action: { type: 'change_status', milestone_id: milestoneId, status: step },
          },
          `milestone ${milestoneId} ${step}`,
        )
      }
    }
  }

  // 3. Tasks: 122 across six projects, every status and priority represented.
  const tasks: SeedTask[] = []
  for (const [projectIndex, plan] of PROJECT_PLANS.entries()) {
    // The plan's own project is the identity; `SEED_PROJECTS` is in a different
    // order, so deriving the task id prefix from its index produced ids that
    // named the wrong project (p-basic tasks were called `t-habits-*`).
    const projectId = plan.id
    const projectMilestones = MILESTONES.filter((milestone) => milestone.projectId === plan.id)

    for (const [index, noun] of plan.nouns.entries()) {
      const id = `t-${projectId.replace(/^p-/, '')}-${String(index + 1).padStart(2, '0')}`
      const status = TASK_STATUSES[(index + projectIndex * 3) % TASK_STATUSES.length]
      const priority = PRIORITY_CYCLE[(index + projectIndex) % PRIORITY_CYCLE.length]
      const labelCount = index % 4
      const labels = plan.labels.slice(0, labelCount)
      const terminal = status === 'done' || status === 'cancelled'

      let deadlineAtMs: number | null = null
      if (!terminal) {
        const slot = (index + projectIndex) % 11
        if (slot === 0) deadlineAtMs = nowMs - 3 * DAY + index * HOUR
        else if (slot === 1) deadlineAtMs = nowMs + 4 * HOUR
        else if (slot === 2) deadlineAtMs = nowMs + (5 + index) * DAY
      }
      const estimatedMinutes = index % 4 === 0 ? null : ((index % 8) + 1) * 15

      const title = `${plan.verb} ${noun}`
      await command(
        '/v1/authoritative/task-commands',
        {
          command_id: `seed-task-${id}`,
          action: { type: 'create', task_id: id, title, project_id: projectId },
        },
        `task ${id} create`,
      )

      const description = DESCRIPTION_TEMPLATES[index % DESCRIPTION_TEMPLATES.length]
      if (description.length > 0 || priority !== null || labels.length > 0) {
        await command(
          '/v1/authoritative/task-commands',
          {
            command_id: `seed-task-${id}-details`,
            action: { type: 'set_details', task_id: id, description, priority, labels },
          },
          `task ${id} details`,
        )
      }

      if (estimatedMinutes !== null || deadlineAtMs !== null) {
        await command(
          '/v1/authoritative/task-commands',
          {
            command_id: `seed-task-${id}-planning`,
            action: {
              type: 'set_planning',
              task_id: id,
              estimated_minutes: estimatedMinutes,
              earliest_start_at_ms: null,
              deadline_at_ms: deadlineAtMs,
            },
          },
          `task ${id} planning`,
        )
      }

      const milestone = projectMilestones.length > 0 && index % 4 === 0 ? projectMilestones[0] : null
      if (milestone) {
        await command(
          '/v1/authoritative/task-commands',
          {
            command_id: `seed-task-${id}-milestone`,
            action: { type: 'assign_milestone', task_id: id, milestone_id: milestone.id },
          },
          `task ${id} milestone`,
        )
      }

      for (const next of STATUS_PATH[status]) {
        await command(
          '/v1/authoritative/task-commands',
          {
            command_id: `seed-task-${id}-status-${next}`,
            action: { type: 'change_status', task_id: id, status: next },
          },
          `task ${id} → ${next}`,
        )
      }

      if (index % 3 === 0) {
        const criteria = [
          `Behaviour covered by a test for ${noun}`,
          'Verified in a real browser with the seeded workspace',
        ]
        for (const [position, text] of criteria.entries()) {
          await command(
            '/v1/authoritative/task-commands',
            {
              command_id: `seed-task-${id}-criterion-${position}`,
              action: {
                type: 'acceptance_criteria',
                task_id: id,
                change: { operation: 'add', criterion_id: `ac-${id}-${position}`, text },
              },
            },
            `task ${id} criterion ${position}`,
          )
        }
        if (index % 6 === 0) {
          await command(
            '/v1/authoritative/task-commands',
            {
              command_id: `seed-task-${id}-criterion-0-done`,
              action: {
                type: 'acceptance_criteria',
                task_id: id,
                change: { operation: 'set_completed', criterion_id: `ac-${id}-0`, completed: true },
              },
            },
            `task ${id} criterion complete`,
          )
        }
      }

      tasks.push({
        id,
        title,
        status,
        projectId,
        milestoneId: milestone?.id ?? null,
        priority,
        labels: [...labels],
        estimatedMinutes,
        deadlineAtMs,
        overdue: deadlineAtMs !== null && deadlineAtMs < nowMs,
        dueToday: deadlineAtMs !== null && deadlineAtMs >= nowMs && deadlineAtMs < nowMs + DAY,
      })
    }
  }

  // 4. Dependencies: every seventh task waits on one two slots earlier, which
  //    gives the readiness/next-action surfaces both blocked and unblocking work.
  for (const project of SEED_PROJECTS) {
    const projectTasks = tasks.filter((task) => task.projectId === project.id)
    for (const [index, task] of projectTasks.entries()) {
      if (index < 2 || index % 7 !== 0) continue
      const prerequisite = projectTasks[index - 2]
      if (prerequisite.status === 'done' || prerequisite.status === 'cancelled') continue
      await command(
        '/v1/authoritative/task-commands',
        {
          command_id: `seed-task-${task.id}-prereq-${prerequisite.id}`,
          action: {
            type: 'add_prerequisite',
            task_id: task.id,
            prerequisite_id: prerequisite.id,
          },
        },
        `task ${task.id} prerequisite`,
      )
    }
  }

  // 5. Knowledge: folders first (a create into a missing folder is rejected),
  //    then markdown documents that link to each other.
  const notes: SeedNote[] = []
  if (extras) {
    for (const path of ['notes', 'notes/decisions', 'notes/research']) {
      await transport
        .post('/v1/authoritative/folder-creates', {
          command_id: `seed-folder-${path.replace(/\//g, '-')}`,
          command: { path },
        })
        .catch((error) => {
          throw withLabel(error, `folder ${path}`, '/v1/authoritative/folder-creates')
        })
    }

    const documents: readonly { id: string; path: string; body: string }[] = [
      {
        id: 'n-orbit-overview',
        path: 'notes/orbit-overview.md',
        body: '# Orbit overview\n\nOrbit is a self-hosted workspace: authoritative state lives in one server, and every\nclient is a view over it.\n\nSee [[n-decision-cookie-sessions]] before changing auth.\n',
      },
      {
        id: 'n-decision-cookie-sessions',
        path: 'notes/decisions/cookie-sessions.md',
        body: '# Decision: cookie sessions\n\nDevice credentials are exchanged once for an HttpOnly cookie. The token never reaches\n`localStorage`, and every mutation carries `x-orbit-csrf: 1`.\n\nRelated: [[n-orbit-overview]]\n',
      },
      {
        id: 'n-decision-markdown-vault',
        path: 'notes/decisions/markdown-vault.md',
        body: '# Decision: the vault owns note bodies\n\nSQLite stores intent and receipts; the file system stores the Markdown. Reads are file\nreads, so external edits are visible immediately.\n',
      },
      {
        id: 'n-research-readiness',
        path: 'notes/research/readiness.md',
        body: '# Readiness notes\n\nA task is actionable only when every prerequisite is finished. The server owns that\njudgement; the client only explains it.\n\n- prerequisite edges are one-directional\n- deadlines are caller-supplied milliseconds\n',
      },
      {
        id: 'n-research-upgrades',
        path: 'notes/research/workflow-upgrades.md',
        body: '# Workflow upgrade safety\n\nUpgrades preview a diff, fingerprints it, and refuse to apply when the fingerprint in the\nrequest no longer matches the plan the human saw.\n',
      },
      {
        id: 'n-i1-acceptance',
        path: 'notes/i1-acceptance.md',
        body: '# I1 acceptance script\n\n1. Sign in with a device token.\n2. Create a project.\n3. Capture a task, edit it, change its status.\n4. Reload: the state is still there.\n',
      },
      {
        id: 'n-learning-plan',
        path: 'notes/learning-plan.md',
        body: '# Learning plan\n\nBundled workflow `learning` gives lessons and assessments. Track progress with\n`[[n-research-readiness|readiness]]`-style notes rather than a private spreadsheet.\n',
      },
      {
        id: 'n-habits-baseline',
        path: 'notes/habits-baseline.md',
        body: '# Habit baseline\n\nStart with three habits and a two week streak target. Anything more is noise.\n',
      },
    ]
    for (const document of documents) {
      await transport
        .post('/v1/authoritative/markdown-commands', {
          command_id: `seed-note-${document.id}`,
          command: { kind: 'Create', note_id: document.id, path: document.path, body: document.body },
        })
        .catch((error) => {
          throw withLabel(error, `note ${document.id}`, '/v1/authoritative/markdown-commands')
        })
      notes.push({ id: document.id, path: document.path, title: document.path })
    }

    // 6. Small deterministic attachments (raw body bytes, metadata in the query).
    //    The SVG and the work-item attachment give the browser a real preview
    //    target and a second owner surface without opening a second upload path.
    const STATUS_CARD = Buffer.from(
      [
        '<svg xmlns="http://www.w3.org/2000/svg" width="480" height="150" viewBox="0 0 480 150">',
        '<rect width="480" height="150" rx="16" fill="#ece9ff"/>',
        '<rect x="24" y="28" width="132" height="12" rx="6" fill="#6750d8"/>',
        '<rect x="24" y="54" width="300" height="8" rx="4" fill="#a99cf0"/>',
        '<rect x="24" y="74" width="252" height="8" rx="4" fill="#c9c1f7"/>',
        '<circle cx="414" cy="75" r="34" fill="#6750d8" opacity="0.18"/>',
        '<path d="M400 75h28M414 61v28" stroke="#6750d8" stroke-width="5" stroke-linecap="round"/>',
        '<text x="24" y="120" font-family="monospace" font-size="12" fill="#6750d8">Fixture attachment</text>',
        '</svg>',
      ].join(''),
      'utf8',
    )
    const attachmentPlans = [
      { ownerKind: 'task', ownerId: tasks[0].id, name: 'acceptance-notes.txt', type: 'text/plain' },
      { ownerKind: 'task', ownerId: tasks[1].id, name: 'repro-steps.txt', type: 'text/plain' },
      { ownerKind: 'task', ownerId: tasks[5].id, name: 'sketch.txt', type: 'text/plain' },
      { ownerKind: 'project', ownerId: PROJECT_IDS.orbit, name: 'i1-scope.txt', type: 'text/plain' },
      { ownerKind: 'project', ownerId: PROJECT_IDS.research, name: 'sources.txt', type: 'text/plain' },
      {
        ownerKind: 'task',
        ownerId: tasks[0].id,
        name: 'status-card.svg',
        type: 'image/svg+xml',
        bytes: STATUS_CARD,
      },
      {
        ownerKind: 'work_item',
        ownerId: 'wi-orbit-feature-ready',
        name: 'review-notes.txt',
        type: 'text/plain',
      },
    ] as const
    for (const [index, attachment] of attachmentPlans.entries()) {
      const id = `a-${String(index + 1).padStart(2, '0')}`
      const bytes =
        'bytes' in attachment
          ? attachment.bytes
          : new TextEncoder().encode(
              `Orbit fixture attachment ${id}\nSeeded deterministically for ${attachment.ownerKind} ${attachment.ownerId}.\n`,
            )
      const query = [
        `command_id=seed-attachment-${id}`,
        `expected_revision=${revision}`,
        `attachment_id=${id}`,
        `activity_id=seed-activity-${id}`,
        `owner_kind=${attachment.ownerKind}`,
        `owner_id=${attachment.ownerId}`,
        `filename=${encodeURIComponent(attachment.name)}`,
        `content_type=${encodeURIComponent(attachment.type)}`,
        `description=${encodeURIComponent('Fixture attachment')}`,
      ].join('&')
      const result = await transport
        .postBytes<{ revision: number }>(`/v1/authoritative/attachment-import?${query}`, bytes, attachment.type)
        .catch((error) => {
          throw withLabel(error, `attachment ${id}`, '/v1/authoritative/attachment-import')
        })
      advance(`attachment ${id}`, result.revision)
    }
  }

  // 7. Agent jobs: one awaiting review (needs attention), one running, one fresh,
  //    and one whole-handoff job in Development that has no review yet (so the
  //    browser can record an approval rather than a conflicting second one).
  const agentJobs: SeedAgentJob[] = []
  if (extras) {
    const orbitInProgress = tasks.filter(
      (task) => task.projectId === PROJECT_IDS.orbit && task.status === 'in_progress',
    )
    const developmentTasks = tasks.filter(
      (task) => task.projectId === PROJECT_IDS.development && task.status === 'in_progress',
    )
    const developmentTask =
      developmentTasks[0] ??
      tasks.find((task) => task.projectId === PROJECT_IDS.development)
    // A separate terminal job, driven through the public command route, so the
    // browser can exercise resume without waiting for an executor.
    const interruptedTask = developmentTasks[1] ?? null
    const jobTasks = [
      ...orbitInProgress.slice(0, 3),
      ...(developmentTask ? [developmentTask] : []),
    ]
    // The launch routes refuse timestamps that move backwards, so the whole
    // sequence is anchored in the past and only ever increases.
    const anchor = nowMs - 4 * HOUR
    // The launch sequence below is one-shot by design (claim tokens and launch
    // revisions are monotonic), so a replay skips jobs that already exist.
    const existing = new Set(
      (
        await transport.get<Array<{ job_id: string }>>('/v1/authoritative/agent-jobs/overview')
      ).map((job) => job.job_id),
    )
    for (const [index, task] of jobTasks.entries()) {
      const jobId = `job-seed-${index + 1}`
      if (existing.has(jobId)) {
        agentJobs.push({
          id: jobId,
          taskId: task.id,
          handoffId: index === 0 || index === 3 ? `h-${jobId}` : null,
        })
        continue
      }
      await transport
        .post('/v1/authoritative/agent-jobs/start', {
          job_id: jobId,
          task_id: task.id,
          agent_kind: index % 2 === 0 ? 'opencode' : 'codex',
          machine_ref: machineRef,
          repository_ref: 'orbit',
          worktree: false,
          at_ms: anchor,
        })
        .catch((error) => {
          throw withLabel(error, `agent job ${jobId}`, '/v1/authoritative/agent-jobs/start')
        })

      let handoffId: string | null = null
      if (index === 0 || index === 3) {
        // Drive the job to `running` through the machine routes so a handoff and
        // a review exist — that is what the attention queue reads.
        const claimToken = `claim-${jobId}`
        const claimed = await transport
          .post<{ job: { revision: number } }>('/v1/authoritative/agent-machines/claim', {
            job_id: jobId,
            expected_revision: 1,
            claim_token: claimToken,
            at_ms: anchor + 1_000,
          })
          .catch((error) => {
            throw withLabel(error, `agent job ${jobId} claim`, '/v1/authoritative/agent-machines/claim')
          })
        await transport
          .post('/v1/authoritative/agent-machines/launch-result', {
            job_id: jobId,
            claim_token: claimToken,
            session_ref: `session-${jobId}`,
            at_ms: anchor + 2_000,
          })
          .catch((error) => {
            throw withLabel(
              error,
              `agent job ${jobId} launch`,
              '/v1/authoritative/agent-machines/launch-result',
            )
          })
        await transport
          .post('/v1/authoritative/agent-machines/observe', {
            job_id: jobId,
            expected_revision: claimed.job.revision,
            claim_token: claimToken,
            session_ref: `session-${jobId}`,
            activity: 'working',
            at_ms: anchor + 3_000,
          })
          .catch((error) => {
            throw withLabel(error, `agent job ${jobId} observe`, '/v1/authoritative/agent-machines/observe')
          })

        handoffId = `h-${jobId}`
        await transport
          .post('/v1/authoritative/agent-jobs/handoffs', {
            job_id: jobId,
            handoff_id: handoffId,
            payload:
              index === 0
                ? {
                    summary: 'Implemented the session cookie flow behind a failing test.',
                    validation: 'unit + browser checks pass locally',
                    next_steps: ['review the CSRF guard', 'confirm cookie flags'],
                  }
                : {
                    summary: 'Added the guarded work-item transition and its refusal.',
                    validation: 'browser checks pass against the deterministic fixture',
                    next_steps: ['approve the guard copy', 'confirm the task stays in progress'],
                  },
            at_ms: anchor + 4_000,
          })
          .catch((error) => {
            throw withLabel(error, `agent job ${jobId} handoff`, '/v1/authoritative/agent-jobs/handoffs')
          })
        if (index === 0) {
          await transport
            .post('/v1/authoritative/agent-jobs/review', {
              job_id: jobId,
              handoff_id: handoffId,
              decision: 'changes_requested',
              reason: 'Add a test for a stale session cookie before this lands.',
              at_ms: anchor + 5_000,
            })
            .catch((error) => {
              throw withLabel(error, `agent job ${jobId} review`, '/v1/authoritative/agent-jobs/review')
            })
        }
      }
      agentJobs.push({ id: jobId, taskId: task.id, handoffId })
    }

    // 8. A terminal job with a durable handoff: starting it, recording the
    //    handoff and interrupting the record through `agent-jobs/command` makes
    //    it resumable. The external session never existed, which is exactly the
    //    case the browser must not misrepresent as launchable.
    if (interruptedTask) {
      const jobId = 'job-seed-5'
      if (existing.has(jobId)) {
        agentJobs.push({ id: jobId, taskId: interruptedTask.id, handoffId: `h-${jobId}` })
      } else {
        await transport
          .post('/v1/authoritative/agent-jobs/start', {
            job_id: jobId,
            task_id: interruptedTask.id,
            agent_kind: 'opencode',
            machine_ref: machineRef,
            repository_ref: 'orbit',
            worktree: false,
            at_ms: anchor + 6_000,
          })
          .catch((error) => {
            throw withLabel(error, `agent job ${jobId}`, '/v1/authoritative/agent-jobs/start')
          })
        await transport
          .post('/v1/authoritative/agent-jobs/handoffs', {
            job_id: jobId,
            handoff_id: `h-${jobId}`,
            payload: {
              summary: 'Interrupted after the handoff was recorded.',
              validation: 'the durable handoff is on record; no runtime session was started',
              next_steps: ['resume as a new job when an executor is available'],
            },
            at_ms: anchor + 7_000,
          })
          .catch((error) => {
            throw withLabel(error, `agent job ${jobId} handoff`, '/v1/authoritative/agent-jobs/handoffs')
          })
        await transport
          .post('/v1/authoritative/agent-jobs/command', {
            command_id: `seed-${jobId}-interrupt`,
            job_id: jobId,
            // `start` reserves queued at 0 and accepts prepare at 1; interrupt
            // fences on the state the start route left behind.
            expected_revision: 1,
            action: 'interrupt',
            at_ms: anchor + 8_000,
          })
          .catch((error) => {
            throw withLabel(error, `agent job ${jobId} interrupt`, '/v1/authoritative/agent-jobs/command')
          })
        agentJobs.push({ id: jobId, taskId: interruptedTask.id, handoffId: `h-${jobId}` })
      }
    }
  }

  const byStatus = Object.fromEntries(TASK_STATUSES.map((status) => [status, 0])) as Record<
    TaskStatus,
    number
  >
  const byProject: Record<string, number> = {}
  for (const task of tasks) {
    byStatus[task.status] += 1
    byProject[task.projectId] = (byProject[task.projectId] ?? 0) + 1
  }

  const pick = (status: TaskStatus) => tasks.filter((task) => task.status === status).map((t) => t.id)
  const byDeadline = (left: SeedTask, right: SeedTask) => {
    if (left.deadlineAtMs === null && right.deadlineAtMs === null) return left.id.localeCompare(right.id)
    if (left.deadlineAtMs === null) return 1
    if (right.deadlineAtMs === null) return -1
    return left.deadlineAtMs - right.deadlineAtMs || left.id.localeCompare(right.id)
  }
  const firstOverdue = tasks.filter((task) => task.overdue).sort(byDeadline)[0]?.id ?? null
  const firstDueToday = tasks.filter((task) => task.dueToday).sort(byDeadline)[0]?.id ?? null
  const orbitTask =
    tasks.find((task) => task.projectId === PROJECT_IDS.orbit && task.status === 'ready') ??
    tasks.find((task) => task.projectId === PROJECT_IDS.orbit) ??
    tasks[0]

  return {
    revision,
    nowMs,
    projects: [...SEED_PROJECTS],
    tasks,
    workItems,
    milestones: [...MILESTONES],
    notes,
    agentJobs,
    ids: {
      b: {
        overdue: tasks.filter((task) => task.overdue).map((task) => task.id),
        dueToday: tasks.filter((task) => task.dueToday).map((task) => task.id),
        ready: pick('ready'),
        blocked: pick('blocked'),
        inProgress: pick('in_progress'),
        inbox: pick('inbox'),
        done: pick('done'),
      },
      orbitTask: orbitTask.id,
      firstOverdue,
      firstDueToday,
    },
    counts: {
      total: tasks.length,
      workItems: workItems.length,
      byStatus,
      byProject,
      overdue: tasks.filter((task) => task.overdue).length,
      dueToday: tasks.filter((task) => task.dueToday).length,
      ready: byStatus.ready,
      blocked: byStatus.blocked,
    },
  }
}

function withLabel(error: unknown, label: string, path: string): Error {
  const message = error instanceof Error ? error.message : String(error)
  return new Error(`seed fixture: ${label} failed at ${path}: ${message}`)
}

/**
 * Bearer-authenticated transport for a running server: the same credentials a
 * CLI or the operator console would use, so seeding never depends on a browser.
 */
export function bearerTransport(options: {
  base: string
  deviceId: string
  token: string
}): SeedTransport {
  const headers = {
    authorization: `Bearer ${options.token}`,
    'x-orbit-device-id': options.deviceId,
  }
  return {
    async post<T>(path: string, body: unknown): Promise<T> {
      const response = await fetch(`${options.base}${path}`, {
        method: 'POST',
        headers: { ...headers, 'content-type': 'application/json' },
        body: JSON.stringify(body),
      })
      return parse<T>(response, path)
    },
    async get<T>(path: string): Promise<T> {
      const response = await fetch(`${options.base}${path}`, {
        headers: { ...headers, accept: 'application/json' },
      })
      return parse<T>(response, path)
    },
    async postBytes<T>(path: string, bytes: Uint8Array, contentType: string): Promise<T> {
      const response = await fetch(`${options.base}${path}`, {
        method: 'POST',
        headers: { ...headers, 'content-type': contentType },
        body: bytes,
      })
      return parse<T>(response, path)
    },
    async revision(): Promise<number> {
      const response = await fetch(`${options.base}/v1/authoritative/projects`, {
        headers: { ...headers, accept: 'application/json' },
      })
      const payload = await parse<{ revision: number }>(response, '/v1/authoritative/projects')
      return payload.revision
    },
  }
}

async function parse<T>(response: Response, path: string): Promise<T> {
  const text = await response.text()
  if (!response.ok) {
    throw new SeedError(path, response.status, text, 'request')
  }
  if (text.length === 0) return undefined as T
  try {
    return JSON.parse(text) as T
  } catch {
    throw new SeedError(path, response.status, text, 'non-JSON response')
  }
}

/** Convenience entry point for dogfooding against a long-lived local server. */
export async function seedWorkspaceForServer(options: {
  base: string
  deviceId: string
  token: string
  nowMs?: number
  extras?: boolean
}): Promise<WorkspaceFixture> {
  return seedWorkspace(bearerTransport(options), {
    nowMs: options.nowMs,
    machineRef: options.deviceId,
    extras: options.extras,
  })
}
