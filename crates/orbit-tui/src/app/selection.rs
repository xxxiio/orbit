//! Cursor and selection bookkeeping shared by every screen.
use super::*;

impl App {
    fn cursor_key(&self) -> Option<String> {
        match self.screen {
            Screen::Home => Some("home".into()),
            Screen::Inbox => Some("inbox".into()),
            Screen::Agents => Some("agents".into()),
            Screen::Projects => None,
            Screen::Project => {
                let project = self.project.as_ref()?;
                let view = project.views.get(project.active_view)?;
                Some(format!("project:{}:{}", project.id, view.id))
            }
        }
    }

    pub(crate) fn sync_cursor(&mut self) {
        let Some(key) = self.cursor_key() else {
            return;
        };
        let count = self.component_count();
        let selected_id = self.selected_item().map(|item| item.id.clone());
        let state = self.cursors.entry(key).or_default();
        state.active_component = self.active_component;
        state.selected.resize(count, 0);
        state.selected_id.resize(count, None);
        state.scroll.resize(count, 0);
        if let Some(index) = state.selected.get_mut(self.active_component) {
            *index = self.selected;
        }
        if let Some(index) = state.selected_id.get_mut(self.active_component) {
            *index = selected_id.clone();
        }
        if let Some(index) = state.scroll.get_mut(self.active_component) {
            *index = self.scroll;
        }
    }

    pub(crate) fn restore_cursor(&mut self) {
        if let Some(key) = self.cursor_key() {
            let count = self.component_count();
            let state = self.cursors.get(&key).cloned().unwrap_or_default();
            self.active_component = state.active_component.min(count.saturating_sub(1));
            let selected = state
                .selected
                .get(self.active_component)
                .copied()
                .unwrap_or(0);
            let selected_id = state
                .selected_id
                .get(self.active_component)
                .cloned()
                .flatten();
            let scroll = state
                .scroll
                .get(self.active_component)
                .copied()
                .unwrap_or(0);
            let len = self
                .current_component()
                .map(|component| component.selection.len())
                .unwrap_or(0);
            let selected = selected_id
                .and_then(|id| {
                    self.current_component().and_then(|component| {
                        component.selection.iter().position(|item| item.id == id)
                    })
                })
                .unwrap_or(selected);
            self.selected = selected.min(len.saturating_sub(1));
            self.scroll = if len == 0 {
                scroll
            } else {
                scroll.min(self.selected)
            };
        }
        self.seed_detail_cache();
    }

    pub fn current_component(&self) -> Option<&ResolvedComponent> {
        self.current_view()?.components.get(self.active_component)
    }

    pub fn component_count(&self) -> usize {
        self.current_view()
            .map(|view| view.components.len())
            .unwrap_or(0)
    }

    pub fn selected_item(&self) -> Option<&SelectionItem> {
        self.current_component()?.selected(self.selected)
    }

    /// Stable identity of the selected row, used for durable navigation state.
    pub fn current_selected_id(&self) -> Option<String> {
        self.selected_item().map(|item| item.id.clone())
    }

    /// Restore the active component, selection and scroll offsets from durable
    /// navigation state without touching domain data.
    ///
    /// Selection prefers a stable identifier (so reordered rows still resolve),
    /// then the saved index, and always lands on a valid row: index `0` for an
    /// empty component. A changed component layout does not lose a row: the id
    /// is resolved across every component of the current view. Scroll offsets
    /// are clamped to the restored selection and only applied when `view_id`
    /// and the saved component index both still describe the active view.
    pub fn restore_selection(
        &mut self,
        selected_id: Option<&str>,
        index: Option<usize>,
        active_component: Option<usize>,
        view_id: Option<&str>,
        scroll: Option<usize>,
        detail_scroll: Option<usize>,
    ) {
        /// Stale detail offsets must not scroll a restored pane out of view.
        const MAX_DETAIL_SCROLL: usize = 500;
        let count = self.component_count();
        let saved_component = active_component
            .unwrap_or(self.active_component)
            .min(count.saturating_sub(1));
        // A saved offset only describes the view it was captured in.
        let same_view = view_id.is_some_and(|id| {
            self.current_view()
                .is_some_and(|view| view.id.as_str() == id)
        });
        // Prefer the saved component (it keeps the saved scroll), then fall
        // back to the owning component anywhere in this view. Never switch
        // views: a row from another view is not a valid restore target.
        let located = selected_id.and_then(|id| {
            let view = self.current_view()?;
            view.components
                .get(saved_component)
                .and_then(|component| {
                    component
                        .selection
                        .iter()
                        .position(|item| item.id == id)
                        .map(|row| (saved_component, row))
                })
                .or_else(|| {
                    view.components
                        .iter()
                        .enumerate()
                        .find_map(|(component_index, component)| {
                            component
                                .selection
                                .iter()
                                .position(|item| item.id == id)
                                .map(|row| (component_index, row))
                        })
                })
        });
        match located {
            Some((component, row)) => {
                self.active_component = component;
                self.selected = row;
                self.scroll = if same_view && component == saved_component {
                    scroll.unwrap_or(0).min(row)
                } else {
                    0
                };
            }
            None => {
                let len = self
                    .current_view()
                    .and_then(|view| view.components.get(saved_component))
                    .map(|component| component.selection.len())
                    .unwrap_or(0);
                self.active_component = saved_component;
                self.selected = index.unwrap_or(0).min(len.saturating_sub(1));
                let scroll = scroll.unwrap_or(0);
                self.scroll = if !same_view {
                    0
                } else if len == 0 {
                    scroll
                } else {
                    scroll.min(self.selected)
                };
            }
        }
        if let Some(detail_scroll) = detail_scroll {
            self.detail_scroll = detail_scroll.min(MAX_DETAIL_SCROLL);
        }
        if count > 0 {
            self.sync_cursor();
        }
        self.seed_detail_cache();
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.screen == Screen::Projects && self.focus == FocusPane::Workspace {
            let max = self.projects.len().saturating_sub(1);
            self.projects_selection = self
                .projects_selection
                .saturating_add_signed(delta)
                .min(max);
            self.seed_detail_cache();
            return;
        }
        match self.focus {
            FocusPane::Sidebar => {
                let max = self.sidebar_len().saturating_sub(1);
                self.sidebar_selection =
                    self.sidebar_selection.saturating_add_signed(delta).min(max);
            }
            FocusPane::Workspace => {
                let len = self
                    .current_component()
                    .map(|component| component.selection.len())
                    .unwrap_or(0);
                if len == 0 {
                    self.scroll = self.scroll.saturating_add_signed(delta);
                    self.selected = 0;
                } else {
                    self.selected = self.selected.saturating_add_signed(delta).min(len - 1);
                    self.detail_scroll = 0;
                }
                // The pane's subject changed: rebuild it here, never in render.
                self.seed_detail_cache();
            }
            FocusPane::Detail => {
                let max = self.detail_max_scroll.get();
                self.detail_scroll = self.detail_scroll.saturating_add_signed(delta).min(max);
            }
        }
    }

    pub fn move_component(&mut self, delta: isize) {
        let count = self.component_count();
        if count == 0 {
            return;
        }
        self.sync_cursor();
        let next = self
            .active_component
            .saturating_add_signed(delta)
            .min(count - 1);
        self.active_component = next;
        if let Some(key) = self.cursor_key()
            && let Some(state) = self.cursors.get_mut(&key)
        {
            state.active_component = next;
        }
        self.restore_cursor();
    }

    pub fn focus_next(&mut self, reverse: bool) {
        let mut panes = vec![FocusPane::Sidebar, FocusPane::Workspace];
        if self.show_detail && self.wide_detail() {
            panes.push(FocusPane::Detail);
        }
        let index = panes
            .iter()
            .position(|pane| *pane == self.focus)
            .unwrap_or(1);
        let next = if reverse {
            (index + panes.len() - 1) % panes.len()
        } else {
            (index + 1) % panes.len()
        };
        self.focus = panes[next];
        if self.focus == FocusPane::Detail {
            self.detail_scroll = 0;
        }
    }

    fn wide_detail(&self) -> bool {
        // The renderer hides the detail pane below the wide breakpoint; the
        // actual width is updated by `set_viewport`.
        self.viewport_width >= crate::theme::WIDE_BREAKPOINT
    }

    pub(crate) fn clamp_component(&mut self) {
        let count = self.component_count();
        self.active_component = self.active_component.min(count.saturating_sub(1));
        let selected = self
            .current_component()
            .map(|component| component.selection.len())
            .unwrap_or(0);
        self.selected = self.selected.min(selected.saturating_sub(1));
        self.scroll = if selected == 0 {
            self.scroll
        } else {
            self.scroll.min(self.selected)
        };
        // Clamping can change the subject after a restore; keep the pane warm.
        self.seed_detail_cache();
    }
}
