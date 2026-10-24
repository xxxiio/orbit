//! Screens, focus, prompts and modal state owned by [`super::App`].
use crate::backend::PendingCommand;
use crate::connected_actions;
use orbit_blueprint::Blueprint;
use orbit_client::{AcceptanceCriterionDto, ActivityDto, AttachmentDto, PendingAttachmentDto};
use orbit_runtime::planning_actions::PlanningActionMenu;
use orbit_ui::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Inbox,
    Projects,
    Agents,
    Project,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPane {
    Sidebar,
    Workspace,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
    Pending,
}

#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub kind: StatusKind,
}

#[derive(Clone, Debug)]
pub struct ProjectState {
    pub id: String,
    pub name: String,
    pub data: orbit_ui::ProjectData,
    pub views: Vec<ResolvedView>,
    pub active_view: usize,
    pub generated: bool,
    pub blueprint_label: String,
    pub stale_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormChoice {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormFieldKind {
    Text,
    /// Markdown-compatible multi-line text. Enter inserts a newline; the form is
    /// saved with Ctrl+S.
    Multiline,
    Integer,
    Boolean,
    Choice(Vec<FormChoice>),
}

#[derive(Clone, Debug)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub value: String,
    pub initial: String,
    /// Caret position as a character index. Only used by text-like fields.
    pub cursor: usize,
    pub kind: FormFieldKind,
    pub required: bool,
    pub hint: String,
}

#[derive(Clone, Debug)]
pub enum FormMode {
    NewProject,
    NewTask {
        project_id: Option<String>,
    },
    EditTask {
        task_id: String,
    },
    /// Compact everyday task edit: title and priority only. Status stays in the
    /// `S` status menu, where terminal transitions get their confirmation.
    QuickEditTask {
        task_id: String,
    },
    EditTaskDetails {
        task_id: String,
    },
    AddCriterion {
        task_id: String,
    },
    EditCriterion {
        task_id: String,
        criterion_id: String,
    },
    AttachFile {
        owner_kind: String,
        owner_id: String,
    },
    ExportAttachment {
        attachment_id: String,
    },
    ChangeWorkflow {
        project_id: String,
    },
    AddDependency {
        task_id: String,
    },
    RemoveDependency {
        task_id: String,
    },
    EditProject {
        project_id: String,
    },
    NewWorkItemKind {
        project_id: String,
    },
    NewWorkItem {
        project_id: String,
        blueprint: Blueprint,
        kind: String,
    },
    EditWorkItem {
        item_id: String,
        revision: u64,
        blueprint: Blueprint,
        kind: String,
    },
    NewKnowledge,
    InstallBlueprint,
    BindBlueprint {
        project_id: String,
    },
    MetricTarget {
        metric_id: Option<String>,
    },
    MetricObservation {
        metric_id: Option<String>,
    },
    BindEvidence {
        project_id: String,
        unbind: bool,
    },
}

#[derive(Clone, Debug)]
pub struct FormState {
    pub title: String,
    pub hint: String,
    pub mode: FormMode,
    pub fields: Vec<FormField>,
    pub focused: usize,
    pub error: Option<String>,
    /// Index of the field that failed validation. When set, the renderer marks
    /// that row and moves focus there instead of showing only a banner.
    pub invalid: Option<usize>,
    pub dirty: bool,
}

impl FormState {
    /// A validation error only describes the field it was raised for, so typing
    /// in that field clears it. Moving the caret to another field keeps it.
    pub fn clear_error_for_focused(&mut self) {
        if self.invalid == Some(self.focused) {
            self.invalid = None;
            self.error = None;
        }
    }
}

/// Submission failure with an optional field key so the form can point at the
/// row that has to change, not only show a banner.
#[derive(Debug)]
pub(crate) struct FormError {
    pub(crate) message: String,
    pub(crate) field: Option<String>,
}

impl FormError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            field: None,
        }
    }

    pub(crate) fn field(key: &str, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            field: Some(key.to_owned()),
        }
    }
}

impl From<String> for FormError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for FormError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

#[derive(Clone, Debug)]
pub enum PendingAction {
    SetTaskStatus {
        task_id: String,
        status: String,
    },
    BlueprintTransition {
        item_id: String,
        to: String,
        revision: u64,
    },
    RecordHabit {
        habit_id: String,
    },
    ApplyPlan,
    RemoveCriterion {
        task_id: String,
        criterion_id: String,
        task_title: String,
    },
    RemoveAttachment {
        attachment_id: String,
        owner_kind: String,
        owner_id: String,
        title: String,
    },
    OpenAttachment {
        attachment_id: String,
        filename: String,
    },
    /// Restore a dirty form after the user declines to discard it.
    DiscardFormChanges {
        form: Box<FormState>,
    },
    /// Queue one reviewed connected intention (PENDING, never accepted).
    QueueIntent {
        intent: connected_actions::Intent,
    },
    /// Stage a reviewed connected batch in one replica transaction.
    QueueBatch {
        intents: Vec<connected_actions::Intent>,
    },
}

/// Ordered acceptance criteria for one task, with a selectable row.
#[derive(Clone, Debug)]
pub struct CriteriaModal {
    pub task_id: String,
    pub task_title: String,
    pub items: Vec<AcceptanceCriterionDto>,
    pub selected: usize,
}

/// Attachments for one owner entity, with a selectable row.
#[derive(Clone, Debug)]
pub struct AttachmentsModal {
    pub owner_kind: String,
    pub owner_id: String,
    pub title: String,
    /// Server-accepted attachments.
    pub items: Vec<AttachmentDto>,
    /// Locally staged uploads; never merged into `items`.
    pub pending: Vec<PendingAttachmentDto>,
    pub selected: usize,
}

impl AttachmentsModal {
    /// Accepted rows first, then staged rows marked `[pending]`.
    pub fn len(&self) -> usize {
        self.items.len() + self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn selected_attachment(&self) -> Option<&AttachmentDto> {
        if self.selected < self.items.len() {
            self.items.get(self.selected)
        } else {
            self.pending
                .get(self.selected - self.items.len())
                .map(|pending| &pending.attachment)
        }
    }

    pub fn selected_is_pending(&self) -> bool {
        self.selected >= self.items.len() && self.selected < self.len()
    }
}

/// Read-only activity history for one entity.
#[derive(Clone, Debug)]
pub struct ActivityModal {
    pub title: String,
    pub items: Vec<ActivityDto>,
    pub scroll: u16,
}

/// Read-only conflict facts plus the user's proposed replacement command.
///
/// The review token is mandatory before anything can be queued: a modal with
/// `review: None` is a fail-closed display, not permission to replace.
#[derive(Clone, Debug)]
pub struct ConflictModal {
    pub command_id: String,
    pub review_lines: Vec<String>,
    pub replacement: String,
    pub error: Option<String>,
    pub review: Option<orbit_client::authority::ConflictReview>,
}

/// What a console submission resolved to. The shell applies it because the
/// modal itself is temporarily moved out of `App` while keys are dispatched.
#[derive(Clone, Debug)]
pub enum ConsoleOutcome {
    /// Read-only command output (sync status, note search, ...).
    Lines(Vec<String>),
    /// Rejected input; nothing was queued and the text is preserved.
    Error(String),
    /// A validated mutation that still needs explicit confirmation.
    Confirm {
        title: String,
        body: String,
        pending: PendingAction,
    },
    /// An explicit conflict review; the replacement is only queued on Enter.
    Conflict(Box<ConflictModal>),
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // Modals are short-lived and moved, never stored in bulk.
pub enum Modal {
    Form(FormState),
    Confirm {
        title: String,
        body: String,
        pending: PendingAction,
    },
    ActionMenu {
        title: String,
        actions: Vec<AvailableAction>,
        /// Actions before this index form the "Suggested" group; the rest are
        /// the complete list. Rendering uses it only for grouping.
        primary: usize,
        selected: usize,
    },
    Palette {
        query: String,
        entries: Vec<PaletteEntry>,
        selected: usize,
    },
    Help,
    Detail(DetailModel),
    Criteria(CriteriaModal),
    Attachments(AttachmentsModal),
    Activity(ActivityModal),
    QuitConfirm {
        resume: Option<Box<Modal>>,
    },
    TodayActions {
        menu: PlanningActionMenu,
        selected: usize,
    },
    Search {
        query: String,
        hits: Vec<SearchHit>,
        selected: usize,
    },
    Message {
        title: String,
        lines: Vec<String>,
    },
    /// Durable queued intentions, never accepted records.
    Pending {
        rows: Vec<PendingCommand>,
        selected: usize,
        detail: String,
        detail_scroll: usize,
    },
    /// Snapshot-bound review of one rejected command plus a replacement input.
    Conflict {
        command_id: String,
        review_lines: Vec<String>,
        replacement: String,
        error: Option<String>,
        /// `None` fails closed: Enter cannot queue without a reviewed conflict.
        review: Option<orbit_client::authority::ConflictReview>,
    },
    /// Connected command console; queued intentions need confirmation.
    Console {
        input: String,
        suggestions: Vec<String>,
        selected: usize,
        output: Vec<String>,
        error: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct PaletteEntry {
    pub label: String,
    pub description: String,
    pub action: PaletteAction,
    pub enabled: bool,
    /// Position in the in-memory recent-destinations MRU, if this entry is one.
    pub recent_rank: Option<usize>,
}

/// One entry in the compact action bar. The full label stays in the action menu
/// and palette; the bar uses a short label so three actions fit.
#[derive(Clone, Debug)]
pub struct CompactAction {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug)]
pub enum PaletteAction {
    Go(Screen),
    OpenProject(String),
    OpenView(usize),
    Execute(AvailableAction),
    Form(FormMode),
    SelectItem {
        component: usize,
        index: usize,
    },
    OpenKnowledge(String),
    /// Inspect the durable pending/conflicted command list.
    Pending,
    /// Open the connected command console.
    Console,
    Quit,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub label: String,
    pub description: String,
    pub action: PaletteAction,
}

#[derive(Clone, Debug)]
pub struct NavigationState {
    pub screen: Screen,
    pub project_id: Option<String>,
    pub active_view: usize,
    pub active_view_id: Option<String>,
    pub focus: FocusPane,
    pub sidebar_selection: usize,
    pub projects_selection: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CursorState {
    pub(crate) active_component: usize,
    pub(crate) selected: Vec<usize>,
    pub(crate) selected_id: Vec<Option<String>>,
    pub(crate) scroll: Vec<usize>,
}
