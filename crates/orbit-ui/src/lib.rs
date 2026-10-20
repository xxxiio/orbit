//! Shared, renderer-neutral presentation contracts for Orbit project views.
//!
//! Layers:
//! - `data`: read-only application-facing project/workspace snapshots.
//! - `binder`: closed data-source queries with bounded filtering and sorting.
//! - `registry`: trusted compiled-in component descriptors.
//! - `resolve`: declarative blueprint views -> typed component data.
//! - `actions`: one action catalogue shared by menus and the command palette.
//! - `model`: resolved view/component/detail data consumed by any renderer.
pub mod actions;
pub mod binder;
pub mod data;
pub mod model;
pub mod registry;
pub mod resolve;
pub mod snapshot;

pub use data::{ProjectData, ViewDataError, ViewDataSource, WorkspaceData};
pub use model::{
    AvailableAction, ComponentBody, DetailModel, ItemKind, LoadStatus, ResolvedComponent,
    ResolvedView, SelectionItem,
};
pub use registry::ComponentRegistry;
pub use resolve::{
    agents_view, default_views, detail_for_component, home_view, inbox_view,
    resolve_blueprint_views, resolve_component, resolve_view,
};
pub use snapshot::{project_from_snapshot, workspace_from_snapshot};
