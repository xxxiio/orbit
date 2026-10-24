//! Stable CLI argument model; public types are re-exported from the crate root.
pub use super::blueprint_commands::{BlueprintCommand, BlueprintSubcommand};
use super::*;

#[derive(Debug, Parser)]
#[command(
    name = "orbit",
    version,
    about = "Orbit local-first personal execution CLI"
)]
pub struct Cli {
    /// Path to the local Orbit SQLite workspace.
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,

    /// Path to the Orbit configuration file (default: ~/.orbit/config.toml).
    #[arg(long, global = true, env = "ORBIT_CONFIG")]
    pub config: Option<PathBuf>,

    /// Emit a stable JSON response instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create the Orbit home layout, configuration and local database.
    Init(InitArgs),
    /// Diagnose executable, configuration, database and optional dependencies.
    Doctor(DoctorArgs),
    /// Discover deterministic planning capabilities and argument contracts.
    Capabilities(CapabilityArgs),
    Task(TaskCommand),
    /// Attach, inspect, export, remove and flush files for an owner entity.
    Attachment(AttachmentCommand),
    /// Manage agent jobs through the Orbit CLI (local-authoritative mode).
    Agent(AgentCommand),
    Project(ProjectCommand),
    /// Manage versioned, declarative project workflows (local authority).
    ///
    /// "workflow" is accepted as a user-facing alias; "blueprint" remains the
    /// stable command name for existing scripts and automation.
    #[command(visible_alias = "workflow")]
    Blueprint(BlueprintCommand),
    Habit(HabitCommand),
    Routine(RoutineCommand),
    /// File-authoritative Markdown notes with durable receipts.
    Note(NoteCommand),
    /// Physical folder navigation and durable folder mutations.
    Folder(FolderCommand),
    Schedule(ScheduleCommand),
    Sync(SyncCommand),
    /// Search across local Orbit entities. Schedule search requires an explicit time window.
    Search(SearchArgs),
    /// Show workspace/storage diagnostics and local entity counts.
    Status,
    /// Show the current actionable queue across planned, ready and in-progress work.
    Today,
    /// Check for or install a verified Orbit GitHub release update.
    Update(UpdateArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum UpdateChannel {
    Stable,
    Prerelease,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Prerelease => "prerelease",
        }
    }
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Report the selected release without downloading or changing anything.
    #[arg(long, default_value_t = false)]
    pub check: bool,
    /// Install an exact release version (accepts v0.2.0 or 0.2.0).
    #[arg(long)]
    pub version: Option<String>,
    /// Release channel used when --version is omitted.
    #[arg(long, value_enum, default_value_t = UpdateChannel::Stable)]
    pub channel: UpdateChannel,
    /// Installation prefix (default: parent of the directory containing this executable).
    #[arg(long)]
    pub prefix: Option<PathBuf>,
    /// Allow installing a release older than the running version.
    #[arg(long, default_value_t = false)]
    pub allow_downgrade: bool,
    /// Reinstall the selected release even when it matches the running version.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AuthorityArg {
    Local,
    Connected,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Local database path to record in the configuration.
    #[arg(long)]
    pub database: Option<PathBuf>,
    /// Overwrite an existing configuration file atomically.
    #[arg(long, default_value_t = false)]
    pub force: bool,
    /// Deployment authority to configure.
    #[arg(long, value_enum)]
    pub authority: Option<AuthorityArg>,
    /// Connected authority server URL (required with --authority connected).
    #[arg(long)]
    pub server: Option<String>,
    /// Connected replica database path (defaults under ~/.orbit).
    #[arg(long)]
    pub replica_database: Option<PathBuf>,
    /// Default agent provider identifier recorded in the configuration.
    #[arg(long)]
    pub agent_provider: Option<String>,
    /// Accepted for scripting; init never prompts.
    #[arg(long, default_value_t = false)]
    pub non_interactive: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Exit non-zero when the report contains errors.
    #[arg(long, default_value_t = false)]
    pub strict: bool,
}

#[derive(Debug, Args)]
pub struct AttachmentCommand {
    #[command(subcommand)]
    pub command: AttachmentSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AttachmentSubcommand {
    /// Import a local regular file and associate it with an owner.
    Add(AttachmentAddArgs),
    /// List accepted attachments and, in connected mode, pending uploads.
    List(AttachmentListArgs),
    /// Show one attachment's metadata and verify its readable content.
    Inspect(AttachmentInspectArgs),
    /// Copy verified content to a new file; existing files are never overwritten.
    Export(AttachmentExportArgs),
    /// Remove an association (connected: pending rows are discarded locally).
    Remove(AttachmentRemoveArgs),
    /// Connected only: flush the outbox and staged attachment uploads.
    Flush,
}

#[derive(Debug, Args)]
pub struct AttachmentAddArgs {
    /// Owner kind: task, project, work_item, knowledge or agent_job.
    #[arg(value_name = "OWNER_KIND")]
    pub owner_kind: String,
    #[arg(value_name = "OWNER_ID")]
    pub owner_id: String,
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, Args)]
pub struct AttachmentListArgs {
    #[arg(value_name = "OWNER_KIND")]
    pub owner_kind: String,
    #[arg(value_name = "OWNER_ID")]
    pub owner_id: String,
}

#[derive(Debug, Args)]
pub struct AttachmentInspectArgs {
    #[arg(value_name = "ID")]
    pub id: String,
}

#[derive(Debug, Args)]
pub struct AttachmentExportArgs {
    #[arg(value_name = "ID")]
    pub id: String,
    #[arg(value_name = "DESTINATION")]
    pub destination: PathBuf,
}

#[derive(Debug, Args)]
pub struct AttachmentRemoveArgs {
    #[arg(value_name = "ID")]
    pub id: String,
}

#[derive(Debug, Args)]
pub struct TaskCommand {
    #[command(subcommand)]
    pub command: TaskSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum TaskSubcommand {
    Add(TaskAddArgs),
    List(TaskListArgs),
    Start(TaskIdArgs),
    Complete(TaskIdArgs),
    Status(TaskStatusArgs),
    Planning(TaskPlanningArgs),
    /// Add a prerequisite task (the named task is blocked until it is done).
    DependsOn(TaskDependencyArgs),
    /// Remove a direct prerequisite edge.
    Unblock(TaskDependencyArgs),
    /// Explain task readiness at an explicit evaluation time.
    Ready(TaskReadyArgs),
    /// Full prerequisite graph, transitive blockers, and downstream impact.
    Graph(TaskReadyArgs),
    /// Project-level critical prerequisites and cross-project blockers.
    ProjectDependencies(TaskProjectDependenciesArgs),
    PortfolioActions(TaskPortfolioActionsArgs),
    PortfolioCapacity(TaskPortfolioCapacityArgs),
    /// Read-only, revision-fenced comparison of up to eight portfolio alternatives.
    CompareScenarios(TaskCompareScenarioArgs),
    /// Turn a chosen scenario into reviewed calendar slots, without writing.
    ReviewScenario(TaskReviewScenarioArgs),
    /// Review calendar feasibility of the baseline and all alternatives.
    ReviewScenarioOptions(TaskReviewScenarioArgs),
    /// Review concrete schedule slots; this command cannot create reservations.
    ReviewSchedule(TaskReviewScheduleArgs),
    /// Execute only the exact reviewed schedule after explicit confirmation.
    ApplyReviewedSchedule(TaskApplyReviewedScheduleArgs),
    /// Read-only cross-project handoffs and outstanding prerequisites.
    ProjectHandoffs(TaskProjectHandoffsArgs),
    /// Actionable work, deterministically ranked by deadline and downstream impact.
    Next(TaskNextArgs),
    /// Deterministic capacity-aware day plan with explicit exclusions.
    DayPlan(TaskDayPlanArgs),
    /// Fit ready tasks into genuine free intervals in accepted schedule blocks.
    SchedulePlan(TaskSchedulePlanArgs),
    /// Explain project workload, overdue work, and dependency bottlenecks.
    Workload(TaskWorkloadArgs),
    /// Explain accepted Today/Next work, scheduling, and project blockers.
    WorkBrief(TaskWorkBriefArgs),
    /// Read-only task drilldown in the Today/Next accepted planning scope.
    PlanningFocus(TaskPlanningFocusArgs),
    /// Safe next-step menu for a task; does not submit mutations.
    PlanningActions(TaskPlanningActionsArgs),
    /// Revalidate a selected task menu action from a JSON file; never executes a write.
    ResolvePlanningAction(TaskResolvePlanningActionArgs),
    HandoffPlanningAction(TaskResolvePlanningActionArgs),
    HandoffPlanningGroupAction(TaskResolvePlanningGroupActionArgs),
    /// Revalidate a selected group menu action from a JSON file.
    ResolvePlanningGroupAction(TaskResolvePlanningGroupActionArgs),
    /// Safe visible next-step menu for a project or milestone.
    PlanningGroupActions(TaskPlanningGroupActionsArgs),
    /// Accepted project and milestone bottlenecks and next actions.
    PlanningGroup(TaskPlanningGroupArgs),
    /// Preview downstream tasks made actionable by completing one task.
    CompletionImpact(TaskCompletionImpactArgs),
    /// Simulate adding or removing a prerequisite without changing accepted state.
    PreviewDependency(TaskPreviewDependencyArgs),
}

#[derive(Debug, Args)]
pub struct TaskDependencyArgs {
    /// Dependent task.
    pub id: String,
    /// Task which must finish first.
    pub prerequisite: String,
}

#[derive(Debug, Args)]
pub struct TaskPreviewDependencyArgs {
    /// Dependent task whose prerequisite list would change.
    pub id: String,
    /// The prerequisite task.
    pub prerequisite: String,
    /// Preview a removal rather than adding this prerequisite.
    #[arg(long)]
    pub remove: bool,
    /// Explicit evaluation time in Unix milliseconds.
    #[arg(long)]
    pub now_ms: i64,
}

#[derive(Debug, Args)]
pub struct TaskReadyArgs {
    /// Evaluation time (Unix milliseconds); deterministic for offline replay.
    #[arg(long)]
    pub now_ms: i64,
    /// Include explanations for currently non-actionable tasks.
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct TaskPortfolioCapacityArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub available_minutes: i64,
    #[arg(long, default_value_t = 20)]
    pub max_tasks: usize,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub include_unassigned: bool,
    #[arg(long)]
    pub include_unsized: bool,
    #[arg(long)]
    pub default_minutes: Option<i64>,
}

#[derive(Debug, Args)]
pub struct TaskCompareScenarioArgs {
    /// JSON file containing ScenarioCompareRequest. No changes are applied.
    #[arg(long)]
    pub input: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct TaskReviewScenarioArgs {
    /// JSON ScenarioExecutionRequest with explicit scenario, window and stable IDs.
    #[arg(long)]
    pub input: PathBuf,
}

#[derive(Debug, Args)]
pub struct TaskReviewScheduleArgs {
    /// JSON ScheduleApprovalRequest containing stable block and activity IDs.
    #[arg(long)]
    pub input: PathBuf,
}

#[derive(Debug, Args)]
pub struct TaskApplyReviewedScheduleArgs {
    /// JSON ApprovedScheduleRequest with reviewed plan and approval token.
    #[arg(long)]
    pub input: PathBuf,
    /// A second explicit gate; file content must also have approved=true.
    #[arg(long, default_value_t = false)]
    pub confirm: bool,
}

#[derive(Debug, Args)]
pub struct TaskPortfolioActionsArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    #[arg(long)]
    pub include_unassigned: bool,
}

#[derive(Debug, Args)]
pub struct TaskProjectDependenciesArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub include_unassigned: bool,
}

#[derive(Debug, Args)]
pub struct TaskProjectHandoffsArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub include_completed: bool,
}

#[derive(Debug, Args)]
pub struct TaskNextArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct TaskWorkloadArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long, default_value_t = 604800000)]
    pub horizon_ms: i64,
}

#[derive(Debug, Args)]
pub struct TaskCompletionImpactArgs {
    pub id: String,
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct TaskWorkBriefArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub default_minutes: Option<i64>,
}

#[derive(Debug, Args)]
pub struct TaskResolvePlanningActionArgs {
    pub id: String,
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub project: Option<String>,
    /// JSON ActionSelection returned from the accepted action menu.
    #[arg(long)]
    pub selection: std::path::PathBuf,
}
#[derive(Debug, Args)]
pub struct TaskResolvePlanningGroupActionArgs {
    pub project: String,
    #[arg(long)]
    pub milestone: Option<String>,
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub selection: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct TaskPlanningActionsArgs {
    pub id: String,
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub project: Option<String>,
}
#[derive(Debug, Args)]
pub struct TaskPlanningGroupActionsArgs {
    pub project: String,
    #[arg(long)]
    pub milestone: Option<String>,
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct TaskPlanningFocusArgs {
    pub id: String,
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value_t = 12)]
    pub max_neighbors: usize,
}

#[derive(Debug, Args)]
pub struct TaskPlanningGroupArgs {
    pub project: String,
    #[arg(long)]
    pub milestone: Option<String>,
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long, default_value_t = 12)]
    pub max_tasks: usize,
}

#[derive(Debug, Args)]
pub struct TaskDayPlanArgs {
    #[arg(long)]
    pub now_ms: i64,
    #[arg(long)]
    pub available_minutes: i64,
    #[arg(long, default_value_t = 20)]
    pub max_tasks: usize,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value_t = false)]
    pub include_unsized: bool,
    #[arg(long)]
    pub default_minutes: Option<i64>,
}

#[derive(Debug, Args)]
pub struct TaskSchedulePlanArgs {
    #[arg(long)]
    pub starts_at_ms: i64,
    #[arg(long)]
    pub ends_at_ms: i64,
    #[arg(long, default_value_t = 20)]
    pub max_tasks: usize,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub default_minutes: Option<i64>,
}

#[derive(Debug, Args)]
pub struct TaskAddArgs {
    pub title: String,

    /// Optional caller-supplied stable ID for idempotent approved remote creates.
    #[arg(long)]
    pub id: Option<String>,

    /// Optional stable project id.
    #[arg(long)]
    pub project: Option<String>,

    /// Optional stable milestone id. Requires --project.
    #[arg(long)]
    pub milestone: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskListArgs {
    #[arg(long)]
    pub status: Option<CliTaskStatus>,

    /// Restrict tasks to a project id.
    #[arg(long)]
    pub project: Option<String>,

    /// Case-insensitive substring matched against task id/title.
    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskIdArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct TaskStatusArgs {
    pub id: String,

    #[arg(value_enum)]
    pub status: CliTaskStatus,
}

#[derive(Debug, Args)]
pub struct TaskPlanningArgs {
    pub id: String,

    #[arg(long)]
    pub estimate_minutes: Option<u32>,

    #[arg(long)]
    pub earliest_start_ms: Option<i64>,

    #[arg(long)]
    pub deadline_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliTaskStatus {
    Inbox,
    Planned,
    Ready,
    InProgress,
    Waiting,
    Blocked,
    Review,
    Deferred,
    Done,
    Cancelled,
}

impl From<CliTaskStatus> for TaskStatus {
    fn from(value: CliTaskStatus) -> Self {
        match value {
            CliTaskStatus::Inbox => Self::Inbox,
            CliTaskStatus::Planned => Self::Planned,
            CliTaskStatus::Ready => Self::Ready,
            CliTaskStatus::InProgress => Self::InProgress,
            CliTaskStatus::Waiting => Self::Waiting,
            CliTaskStatus::Blocked => Self::Blocked,
            CliTaskStatus::Review => Self::Review,
            CliTaskStatus::Deferred => Self::Deferred,
            CliTaskStatus::Done => Self::Done,
            CliTaskStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Args)]
pub struct ProjectCommand {
    #[command(subcommand)]
    pub command: ProjectSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ProjectSubcommand {
    Add(ProjectAddArgs),
    List(ProjectListArgs),
    Status(ProjectStatusArgs),
    Configure(ProjectConfigureArgs),
    Criterion(ProjectCriterionArgs),
    Health(ProjectHealthArgs),
    /// Manage habit/metric evidence tied to this project.
    Evidence(ProjectEvidenceArgs),
    /// Define metric targets and record time-stamped observations (local-only).
    Metric(ProjectMetricArgs),
}

#[derive(Debug, Args)]
pub struct ProjectEvidenceArgs {
    #[command(subcommand)]
    pub command: ProjectEvidenceSubcommand,
}
#[derive(Debug, Subcommand)]
pub enum ProjectEvidenceSubcommand {
    List {
        project_id: String,
    },
    Bind {
        project_id: String,
        #[arg(value_enum)]
        kind: CliEvidenceKind,
        evidence_id: String,
    },
    Unbind {
        project_id: String,
        #[arg(value_enum)]
        kind: CliEvidenceKind,
        evidence_id: String,
    },
    /// Evaluate bound habits and metrics in an explicit reporting window.
    Report {
        project_id: String,
        #[arg(long)]
        start_ms: i64,
        #[arg(long)]
        end_ms: i64,
        #[arg(long)]
        as_of_ms: i64,
    },
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliEvidenceKind {
    Habit,
    Metric,
}

#[derive(Debug, Args)]
pub struct ProjectMetricArgs {
    #[command(subcommand)]
    pub command: ProjectMetricSubcommand,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliMetricDirection {
    AtLeast,
    AtMost,
}
#[derive(Debug, Subcommand)]
pub enum ProjectMetricSubcommand {
    Target {
        metric_id: String,
        baseline: i64,
        target: i64,
        #[arg(value_enum)]
        direction: CliMetricDirection,
    },
    Observe {
        metric_id: String,
        value: i64,
        #[arg(long)]
        observed_at_ms: i64,
    },
}

#[derive(Debug, Args)]
pub struct ProjectAddArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct ProjectListArgs {
    #[arg(long)]
    pub status: Option<CliProjectStatus>,

    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectStatusArgs {
    pub id: String,

    #[arg(value_enum)]
    pub status: CliProjectStatus,
}

#[derive(Debug, Args)]
pub struct ProjectConfigureArgs {
    pub id: String,

    #[arg(long, value_enum)]
    pub mode: CliProjectMode,

    #[arg(long)]
    pub objective: Option<String>,

    /// Repeat to select supported progress signals. If omitted, mode defaults are used.
    #[arg(long = "signal", value_enum)]
    pub signals: Vec<CliProjectProgressSignal>,

    /// Required success criterion in key=description form. Repeatable.
    #[arg(long = "criterion")]
    pub criteria: Vec<String>,

    /// Optional success criterion in key=description form. Repeatable.
    #[arg(long = "optional-criterion")]
    pub optional_criteria: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ProjectCriterionArgs {
    pub id: String,
    pub key: String,
    #[arg(value_enum)]
    pub status: CliSuccessCriterionStatus,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliProjectMode {
    Standard,
    Learning,
    HabitDriven,
    MetricTarget,
    Maintenance,
    Custom,
}
impl From<CliProjectMode> for ProjectMode {
    fn from(value: CliProjectMode) -> Self {
        match value {
            CliProjectMode::Standard => ProjectMode::Standard,
            CliProjectMode::Learning => ProjectMode::Learning,
            CliProjectMode::HabitDriven => ProjectMode::HabitDriven,
            CliProjectMode::MetricTarget => ProjectMode::MetricTarget,
            CliProjectMode::Maintenance => ProjectMode::Maintenance,
            CliProjectMode::Custom => ProjectMode::Custom,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliProjectProgressSignal {
    Milestones,
    Tasks,
    Blockers,
    Forecast,
    HabitConsistency,
    TimeInvested,
    LearningEvidence,
    MetricValue,
    KnowledgeEvidence,
}
impl From<CliProjectProgressSignal> for ProjectProgressSignal {
    fn from(value: CliProjectProgressSignal) -> Self {
        match value {
            CliProjectProgressSignal::Milestones => ProjectProgressSignal::Milestones,
            CliProjectProgressSignal::Tasks => ProjectProgressSignal::Tasks,
            CliProjectProgressSignal::Blockers => ProjectProgressSignal::Blockers,
            CliProjectProgressSignal::Forecast => ProjectProgressSignal::Forecast,
            CliProjectProgressSignal::HabitConsistency => ProjectProgressSignal::HabitConsistency,
            CliProjectProgressSignal::TimeInvested => ProjectProgressSignal::TimeInvested,
            CliProjectProgressSignal::LearningEvidence => ProjectProgressSignal::LearningEvidence,
            CliProjectProgressSignal::MetricValue => ProjectProgressSignal::MetricValue,
            CliProjectProgressSignal::KnowledgeEvidence => ProjectProgressSignal::KnowledgeEvidence,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliSuccessCriterionStatus {
    Pending,
    Satisfied,
    Waived,
}
impl From<CliSuccessCriterionStatus> for SuccessCriterionStatus {
    fn from(value: CliSuccessCriterionStatus) -> Self {
        match value {
            CliSuccessCriterionStatus::Pending => SuccessCriterionStatus::Pending,
            CliSuccessCriterionStatus::Satisfied => SuccessCriterionStatus::Satisfied,
            CliSuccessCriterionStatus::Waived => SuccessCriterionStatus::Waived,
        }
    }
}

#[derive(Debug, Args)]
pub struct ProjectHealthArgs {
    pub id: String,

    /// Recent-activity lookback used by the transparent health projection.
    #[arg(long, default_value_t = 7 * 24 * 60)]
    pub lookback_minutes: u64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliProjectStatus {
    Proposed,
    Active,
    OnHold,
    Completed,
    Cancelled,
    Archived,
}

impl From<CliProjectStatus> for ProjectStatus {
    fn from(value: CliProjectStatus) -> Self {
        match value {
            CliProjectStatus::Proposed => Self::Proposed,
            CliProjectStatus::Active => Self::Active,
            CliProjectStatus::OnHold => Self::OnHold,
            CliProjectStatus::Completed => Self::Completed,
            CliProjectStatus::Cancelled => Self::Cancelled,
            CliProjectStatus::Archived => Self::Archived,
        }
    }
}

#[derive(Debug, Args)]
pub struct HabitCommand {
    #[command(subcommand)]
    pub command: HabitSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum HabitSubcommand {
    Add(HabitAddArgs),
    List(HabitListArgs),
    Pause(HabitIdArgs),
    Resume(HabitIdArgs),
    Check(HabitCheckArgs),
    History(HabitIdArgs),
}

#[derive(Debug, Args)]
pub struct HabitAddArgs {
    pub name: String,

    /// Weekly target from 1 through 7. Omit for a daily habit.
    #[arg(long)]
    pub weekly_target: Option<u8>,
}

#[derive(Debug, Args)]
pub struct HabitIdArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct HabitCheckArgs {
    pub id: String,

    /// Optional stable occurrence identity for idempotent remote execution.
    #[arg(long)]
    pub occurrence_id: Option<String>,

    /// Completion timestamp. Defaults to the current clock.
    #[arg(long)]
    pub at_ms: Option<i64>,

    /// Optional time spent on this occurrence.
    #[arg(long)]
    pub duration_minutes: Option<u32>,
}

#[derive(Debug, Args)]
pub struct HabitListArgs {
    #[arg(long, conflicts_with = "paused")]
    pub active: bool,

    #[arg(long, conflicts_with = "active")]
    pub paused: bool,

    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Debug, Args)]
pub struct RoutineCommand {
    #[command(subcommand)]
    pub command: RoutineSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum RoutineSubcommand {
    Add(RoutineAddArgs),
    List,
    AddHabit(RoutineHabitArgs),
    RemoveHabit(RoutineHabitArgs),
}

#[derive(Debug, Args)]
pub struct RoutineAddArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct RoutineHabitArgs {
    pub id: String,
    pub habit_id: String,
}

#[derive(Debug, Args)]
pub struct ScheduleCommand {
    #[command(subcommand)]
    pub command: ScheduleSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ScheduleSubcommand {
    Add(ScheduleAddArgs),
    List(ScheduleListArgs),
    Reschedule(ScheduleRescheduleArgs),
}

#[derive(Debug, Args)]
pub struct ScheduleAddArgs {
    pub title: String,

    #[arg(long)]
    pub start_ms: i64,

    #[arg(long)]
    pub duration_minutes: u32,

    /// Make the block immutable to planner/user rescheduling.
    #[arg(long, conflicts_with = "movable")]
    pub fixed: bool,

    /// Explicit movable marker. Movable is the default.
    #[arg(long, conflicts_with = "fixed")]
    pub movable: bool,

    /// Schedule an existing task.
    #[arg(long, conflicts_with = "habit")]
    pub task: Option<String>,

    /// Schedule an existing habit.
    #[arg(long, conflicts_with = "task")]
    pub habit: Option<String>,
}

#[derive(Debug, Args)]
pub struct ScheduleListArgs {
    #[arg(long)]
    pub from_ms: i64,

    #[arg(long)]
    pub to_ms: i64,

    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Debug, Args)]
pub struct ScheduleRescheduleArgs {
    pub id: String,

    #[arg(long)]
    pub start_ms: i64,
}

#[derive(Debug, Args)]
pub struct SyncCommand {
    #[command(subcommand)]
    pub command: SyncSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SyncSubcommand {
    /// Direct server-authoritative task and project commands.
    Authority(SyncAuthorityArgs),
    /// Native server-authoritative offline cache and command queue.
    Replica(SyncReplicaArgs),
}

#[derive(Debug, Args)]
pub struct SyncReplicaArgs {
    /// SQLite file storing ONLY accepted snapshots and pending intentions.
    #[arg(long)]
    pub db: PathBuf,
    /// Server base URL. Replica is permanently bound to this server.
    #[arg(long)]
    pub server: String,
    /// Device authentication token; not required to queue/view while offline.
    #[arg(long, env = "ORBIT_SYNC_HTTP_TOKEN")]
    pub token: Option<String>,
    /// Enrolled device identity for authenticated remote operations.
    #[arg(long, env = "ORBIT_AUTHORITY_DEVICE")]
    pub device: Option<String>,
    #[command(subcommand)]
    pub action: ReplicaAction,
}

#[derive(Debug, Subcommand)]
pub enum ReplicaAction {
    /// Replay pending operations, refresh accepted state and mirror notes.
    Reconnect,
    /// Fetch the six-family, server-accepted snapshot atomically.
    Refresh,
    /// Read last accepted snapshot, including when disconnected.
    Snapshot,
    /// Download all accepted Markdown notes to the offline replica, atomically.
    SyncMarkdown,
    /// Search previously accepted Markdown notes while offline.
    SearchMarkdown {
        query: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// List all cached accepted Markdown identities while offline.
    CachedMarkdown {
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Queue a validated JSON action; not visible in accepted snapshots yet.
    Queue {
        #[arg(long)]
        command_id: String,
        #[arg(long, value_enum)]
        kind: ReplicaKind,
        /// JSON object matching the server's command action contract.
        #[arg(long)]
        action_json: String,
    },
    /// Preview an offline, reviewed multi-domain JSON workflow manifest.
    PreviewWorkflow {
        #[arg(long)]
        file: PathBuf,
    },
    /// Stage a reviewed manifest atomically; does not contact the server.
    StageWorkflow {
        #[arg(long)]
        file: PathBuf,
    },
    /// Submit pending intentions in order, preserving retry identities.
    Flush,
    /// Inspect unresolved commands, including frozen revisions and conflicts.
    Pending,
    /// Reconnect, refresh accepted notes, and observe whether clients have converged.
    Converge,
    /// Compare replica with server's current accepted revision and fingerprint.
    ObserveConvergence,
    /// Refresh server history before issuing a conflict review token.
    ReviewConflictOnline {
        #[arg(long)]
        command_id: String,
    },
    /// Review a rejected intention and the exact accepted history observed.
    ReviewConflict {
        #[arg(long)]
        command_id: String,
    },
    /// Resolve only after review; refuses to use a stale authoritative state.
    ResolveReviewed {
        #[arg(long)]
        review_file: PathBuf,
        #[arg(long)]
        replacement_id: String,
        #[arg(long, value_enum)]
        kind: ReplicaKind,
        #[arg(long)]
        action_json: String,
    },
    /// Atomically stage a file of snapshot-bound reviewed conflict resolutions.
    /// Every item must be based on the same accepted server history.
    ResolveReviewedBatch {
        #[arg(long)]
        file: PathBuf,
    },
    /// Per-domain offline queue state plus actionable synchronization blockers.
    WorkspaceSync,
    /// Observe the same domain queue report against the current live server.
    ObserveWorkspaceSync,
    /// Actionable offline synchronization health; never claims current server agreement.
    SyncHealth,
    /// Compare actionable synchronization health with the live authority.
    ObserveSyncHealth,
    /// Inspect accepted revision, unresolved queue, and local replica integrity.
    Convergence,
    /// Inspect durable state and receipt for an exact command ID, including accepted commands.
    Receipt {
        #[arg(long)]
        command_id: String,
    },
    /// Read a specific accepted domain family without including pending intentions.
    Accepted {
        #[arg(long, value_enum)]
        family: AcceptedFamily,
    },
    /// Audit global queue ordering and receipt consistency without network access.
    Audit,
    /// Show last accepted revision and pending command count.
    Status,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ReplicaKind {
    Project,
    Task,
    Evidence,
    Schedule,
    Routine,
    Milestone,
    Strategy,
    Markdown,
    FolderCreate,
    FolderMove,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AcceptedFamily {
    Projects,
    Tasks,
    Habits,
    HabitOccurrences,
    Metrics,
    MetricObservations,
    Schedule,
    Routines,
    Milestones,
    Strategies,
}
impl AcceptedFamily {
    pub fn key(self) -> &'static str {
        match self {
            Self::Projects => "projects",
            Self::Tasks => "tasks",
            Self::Habits => "habits",
            Self::HabitOccurrences => "habit_occurrences",
            Self::Metrics => "metrics",
            Self::MetricObservations => "metric_observations",
            Self::Schedule => "schedule",
            Self::Routines => "routines",
            Self::Milestones => "milestones",
            Self::Strategies => "strategies",
        }
    }
}

#[derive(Debug, Args)]
pub struct SyncAuthorityArgs {
    /// Authenticated Orbit server URL (HTTPS or loopback HTTP).
    #[arg(long)]
    pub server: String,
    /// Enrolled device identifier.
    #[arg(long)]
    pub device: String,
    /// Token for the enrolled device; do not pass credentials in URLs.
    #[arg(long, env = "ORBIT_SYNC_HTTP_TOKEN")]
    pub token: String,
    #[command(subcommand)]
    pub action: AuthorityTaskAction,
}

#[derive(Debug, Subcommand)]
pub enum AuthorityTaskAction {
    /// Read an accepted full-domain snapshot directly from the authority server.
    Snapshot,
    /// Submit a typed lifecycle command to the authoritative server. Unlike `replica queue`,
    /// this requires connectivity and returns a server receipt immediately.
    DomainCommand {
        #[arg(long, value_enum)]
        kind: DirectDomainKind,
        #[arg(long)]
        command_id: String,
        #[arg(long)]
        revision: i64,
        /// One JSON action matching the selected canonical domain command schema.
        #[arg(long)]
        action_json: String,
    },
    /// Read the committed lifecycle records directly from the authority server.
    DomainRead {
        #[arg(long, value_enum)]
        kind: DirectDomainKind,
    },
    /// Read server-committed task state and its revision.
    Tasks,
    /// List server-owned projects and their authoritative revision.
    Projects,
    /// Create a canonical project, optionally pinning a workflow at creation.
    ProjectCreate {
        #[arg(long)]
        command_id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        /// Workflow id to pin at creation (defaults to Basic).
        #[arg(long)]
        workflow: Option<String>,
    },
    /// Create a task using the expected server revision.
    Create {
        #[arg(long)]
        command_id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        id: String,
        #[arg(long)]
        title: String,
        /// Existing authoritative project; omit for a standalone task.
        #[arg(long)]
        project_id: Option<String>,
    },
    /// Transition a task using the expected server revision.
    Status {
        #[arg(long)]
        command_id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        id: String,
        #[arg(long)]
        status: String,
    },
}

/// Rich domain families with canonical typed server-side action contracts.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum DirectDomainKind {
    Schedule,
    Routine,
    Milestone,
    Strategy,
}
impl DirectDomainKind {
    pub fn commands_path(self) -> &'static str {
        match self {
            Self::Schedule => "schedule-commands",
            Self::Routine => "routine-commands",
            Self::Milestone => "milestone-commands",
            Self::Strategy => "strategy-commands",
        }
    }
    pub fn reads_path(self) -> &'static str {
        match self {
            Self::Schedule => "schedule",
            Self::Routine => "routines",
            Self::Milestone => "milestones",
            Self::Strategy => "strategies",
        }
    }
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Case-insensitive substring matched against titles/names and stable ids.
    pub query: String,

    /// Optional schedule window start. Must be supplied together with --to-ms.
    #[arg(long, requires = "to_ms")]
    pub from_ms: Option<i64>,

    /// Optional schedule window end. Must be supplied together with --from-ms.
    #[arg(long, requires = "from_ms")]
    pub to_ms: Option<i64>,
}

#[derive(Debug, Args)]
pub struct CapabilityArgs {
    /// Exact stable capability ID.
    #[arg(long)]
    pub id: Option<String>,
    /// Only read operations on accepted state.
    #[arg(long)]
    pub accepted_only: bool,
}

/// Agent commands deliberately use the same top-level CLI, deployment guard,
/// JSON formatting, and exit handling as the rest of Orbit.
#[derive(Debug, Args)]
pub struct AgentCommand {
    #[command(subcommand)]
    pub command: AgentSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AgentSubcommand {
    /// Reserve, prepare and start an agent from an Orbit task (local mode).
    Start {
        task_id: String,
        /// Agent provider identifier; defaults to configuration or `codex`.
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value = "local")]
        machine: String,
        #[arg(long)]
        repository: PathBuf,
        /// Create an isolated Git worktree through Herdr before starting the agent.
        #[arg(long, default_value_t = false)]
        worktree: bool,
    },
    /// Dispatch an already prepared job. Requires a durable launch claim token.
    Launch {
        job_id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        claim_token: String,
        #[arg(long)]
        at_ms: i64,
    },
    /// Inspect a previously launched agent; never launches a second session.
    Inspect {
        job_id: String,
        #[arg(long)]
        revision: i64,
        #[arg(long)]
        at_ms: i64,
    },
    /// Reconcile all claimed local agent sessions and capture reviewable evidence.
    Sweep {
        #[arg(long, default_value_t = 20)]
        max_commits: usize,
    },
    /// Queue a connected agent launch for explicit replay when online (no Herdr dispatch yet).
    QueueStart {
        task_id: String,
        /// Agent provider identifier; defaults to configuration or `codex`.
        #[arg(long)]
        agent: Option<String>,
        #[arg(long, default_value = "local")]
        machine: String,
        #[arg(long)]
        repository: PathBuf,
        #[arg(long, default_value_t = false)]
        worktree: bool,
    },
    /// Queue a successor job based on an accepted previous handoff.
    QueueResume { job_id: String },
    /// Queue a message to an existing connected agent session.
    QueueContinue {
        job_id: String,
        #[arg(long)]
        message: String,
    },
    /// Queue a durable agent handoff (copies JSON into the outbox immediately).
    QueueHandoff {
        job_id: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Queue a review decision; never marks the task complete.
    QueueReview {
        job_id: String,
        #[arg(long)]
        handoff_id: String,
        #[arg(long, default_value = "approved")]
        decision: String,
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// Inspect durable pending agent commands without contacting the server.
    Pending,
    /// Submit queued agent commands to the connected authority, preserving original IDs.
    Flush,
    /// Show a job's persisted state without contacting its runtime.
    Status { job_id: String },
    /// Save a structured JSON handoff. Required fields: summary, validation, next_steps.
    Handoff {
        job_id: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Send an explicit continuation to an existing Herdr session (local authority).
    /// Never starts a second agent. Delivery errors are ambiguous; do not auto-retry.
    Continue {
        job_id: String,
        #[arg(long)]
        message: String,
    },
    /// Retrieve durable handoffs for a job (no live agent required).
    Handoffs { job_id: String },
    /// Explicitly complete the Orbit job record of a non-terminal local job.
    /// Preserves handoffs, never completes the Orbit task, and does not stop
    /// the external session.
    Complete {
        job_id: String,
        /// Optional operator note recorded on the completion event.
        #[arg(long, default_value = "")]
        summary: String,
    },
    /// Interrupt the Orbit job record of a non-terminal local job. Handoffs and
    /// the Orbit task are untouched; the external Herdr session is not stopped
    /// and may keep running.
    Interrupt {
        job_id: String,
        /// Optional operator reason recorded on the interrupt event.
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// Build a durable resume brief from the job, latest handoff and review; does not launch.
    ResumeContext { job_id: String },
    /// Start a new agent job using a terminal predecessor and its durable handoff.
    Resume { job_id: String },
    /// Capture read-only Git evidence into a durable handoff for a local repository.
    Capture {
        job_id: String,
        #[arg(long, default_value_t = 20)]
        max_commits: usize,
    },
    /// Approve or request changes on the latest persisted handoff (does not close task).
    Review {
        job_id: String,
        #[arg(long)]
        handoff_id: String,
        #[arg(long)]
        reviewer: String,
        #[arg(long, default_value = "approved")]
        decision: String,
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// Read the job's recorded review decision.
    ReviewStatus { job_id: String },
    /// Deterministic portfolio-wide catch-up with handoffs and review decisions.
    Overview,
    /// List jobs that require attention in this local workspace.
    Attention,
}
