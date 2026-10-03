//! Protocol lifecycle: initialization, shutdown, configuration, workspace
//! folders, and watched files (`notification`), and document open, change,
//! save, and close indexing with diagnostic publication (`indexing`).

pub mod indexing;
pub mod notification;
