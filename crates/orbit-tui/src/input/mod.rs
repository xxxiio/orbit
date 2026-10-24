//! Key dispatch. Modal key handling takes precedence over the shortcut
//! dispatch in `dispatch`, which also owns prefix chords.
mod dispatch;
mod modal;
mod text;

pub(crate) use dispatch::handle_key;
pub(crate) use modal::handle_modal;
