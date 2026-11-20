//! Method inputs and semantic lookup context supplied before traversal.

use crate::core::{FullyQualifiedName, RubyMethod, RubyType};
use crate::inference::semantics::Semantics;
use crate::inference::type_tracker::TypeTracker;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use ustr::Ustr;

#[derive(Default)]
pub(in crate::inference::type_tracker) struct MethodContext {
    /// Explicit parameter contracts for the method being tracked. They seed
    /// the flow environment before the body is visited; a later assignment can
    /// still replace or invalidate that proof normally.
    pub(in crate::inference::type_tracker) parameter_types: HashMap<String, RubyType>,
    /// Parameters whose contract is documented on the method itself, in the
    /// file being tracked. A contract read from a signature file is a
    /// snapshot of another file and is left out.
    pub(in crate::inference::type_tracker) same_file_parameters: HashSet<String>,
    /// Current class/module context for resolving implicit self
    pub(in crate::inference::type_tracker) class: Option<FullyQualifiedName>,
    /// Current method context for resolving `super`.
    pub(in crate::inference::type_tracker) method: Option<RubyMethod>,
    /// The namespace side whose objects run the method being tracked. An
    /// instance variable the method returns belongs to it.
    pub(in crate::inference::type_tracker) instance_variable_owner: Option<FullyQualifiedName>,
    /// What this file's own assignments prove about that owner's variables.
    /// Provisional: other files may assign them too, so every use is
    /// recorded as an observation for the engine to confirm.
    pub(in crate::inference::type_tracker) instance_variable_types: HashMap<Ustr, RubyType>,
    /// For each `@name ||= value` of the method, by where it starts: its
    /// number in the file, and what the file's other assignments of the
    /// variable prove. Provisional in the same way.
    pub(in crate::inference::type_tracker) instance_variable_memos: HashMap<usize, (u32, RubyType)>,
}

#[derive(Default)]
pub(in crate::inference::type_tracker) struct AnalysisContext {
    /// Read-only project semantics for mid-walk reads; see [`Semantics`].
    pub(in crate::inference::type_tracker) project: Option<Arc<dyn Semantics>>,
    /// Same-file method return facts already collected before this method.
    pub(in crate::inference::type_tracker) method_returns: HashMap<FullyQualifiedName, RubyType>,
    /// Same-file methods whose complete current-pass declaration set proves
    /// public explicit-receiver access. This lets return inference use local
    /// method results without guessing through visibility before engine facts
    /// are installed.
    pub(in crate::inference::type_tracker) public_methods: Arc<HashSet<FullyQualifiedName>>,
    /// Same-file superclass edges already collected before this method.
    pub(in crate::inference::type_tracker) superclasses:
        HashMap<FullyQualifiedName, FullyQualifiedName>,
    /// Same-file methods that contain `yield`, keyed by method FQN.
    pub(in crate::inference::type_tracker) method_yields:
        HashMap<FullyQualifiedName, Vec<RubyType>>,
    /// Same-file methods eligible to become return-equation dependencies.
    pub(in crate::inference::type_tracker) method_candidates: Arc<HashSet<FullyQualifiedName>>,
}

impl TypeTracker {
    pub(crate) fn with_semantics(mut self, project: Arc<dyn Semantics>) -> Self {
        self.analysis.project = Some(project);
        self
    }

    pub fn with_local_method_returns(
        mut self,
        local_method_returns: HashMap<FullyQualifiedName, RubyType>,
    ) -> Self {
        self.analysis.method_returns = local_method_returns;
        self
    }

    pub(crate) fn with_local_public_method_candidates(
        mut self,
        local_public_method_candidates: Arc<HashSet<FullyQualifiedName>>,
    ) -> Self {
        self.analysis.public_methods = local_public_method_candidates;
        self
    }

    pub fn with_local_superclasses(
        mut self,
        local_superclasses: HashMap<FullyQualifiedName, FullyQualifiedName>,
    ) -> Self {
        self.analysis.superclasses = local_superclasses;
        self
    }

    pub(crate) fn with_parameter_types(
        mut self,
        parameter_types: HashMap<String, RubyType>,
    ) -> Self {
        self.context.parameter_types = parameter_types;
        self
    }

    /// Name the parameters whose contract the tracked file itself documents.
    pub(crate) fn with_same_file_parameters(mut self, names: HashSet<String>) -> Self {
        self.context.same_file_parameters = names;
        self
    }

    pub fn with_yield_param_types(
        mut self,
        yield_param_types_by_method: HashMap<FullyQualifiedName, Vec<RubyType>>,
    ) -> Self {
        self.analysis.method_yields = yield_param_types_by_method;
        self
    }

    /// Set the current class/module context for resolving implicit self
    pub fn set_current_class(&mut self, fqn: Option<FullyQualifiedName>) {
        self.context.class = fqn;
    }

    /// Set the namespace side that owns instance variables read by the
    /// method being tracked, and what this file alone proves about them.
    pub fn set_instance_variables(
        &mut self,
        owner: Option<FullyQualifiedName>,
        file_local_types: HashMap<Ustr, RubyType>,
        memos: HashMap<usize, (u32, RubyType)>,
    ) {
        self.context.instance_variable_types = file_local_types;
        self.context.instance_variable_memos = memos;
        self.context.instance_variable_owner = owner;
    }
}
