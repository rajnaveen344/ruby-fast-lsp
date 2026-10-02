//! `ProjectRuntimeState`: the runtime a project selected, the Ruby version it
//! detected, and its JRuby add-on. Each value has its own lock because each
//! changes on its own transition; none is held under the engine guard.
use crate::environment::config::runtime::SelectedRuntimeDescriptor;
use crate::loader::jruby_add_on::JrubyAddOn;
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct ProjectRuntimeState {
    selected: Arc<RwLock<Option<SelectedRuntimeDescriptor>>>,
    ruby_version: Arc<RwLock<Option<String>>>,
    jruby: Arc<RwLock<Option<JrubyAddOn>>>,
}

impl ProjectRuntimeState {
    pub fn selected(&self) -> &RwLock<Option<SelectedRuntimeDescriptor>> {
        &self.selected
    }

    pub fn ruby_version(&self) -> &RwLock<Option<String>> {
        &self.ruby_version
    }

    /// The classpath fingerprint of the project's JRuby add-on, if any.
    pub fn classpath_fingerprint(&self) -> Option<String> {
        self.jruby
            .read()
            .as_ref()
            .map(|add_on| add_on.classpath_fingerprint().to_string())
    }

    pub(crate) fn jruby_add_on(&self) -> &RwLock<Option<JrubyAddOn>> {
        &self.jruby
    }
}
