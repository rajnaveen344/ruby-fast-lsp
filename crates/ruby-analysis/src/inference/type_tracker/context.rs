//! Method inputs and semantic lookup context supplied before traversal.

use crate::core::{FullyQualifiedName, RubyMethod, RubyType};
use crate::engine::{AnalysisEngine, AnalysisQueryCache};
use crate::inference::type_tracker::TypeTracker;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Default)]
pub(in crate::inference::type_tracker) struct MethodContext {
    /// Explicit parameter contracts for the method being tracked. They seed
    /// the flow environment before the body is visited; a later assignment can
    /// still replace or invalidate that proof normally.
    pub(in crate::inference::type_tracker) parameter_types: HashMap<String, RubyType>,
    /// Current class/module context for resolving implicit self
    pub(in crate::inference::type_tracker) class: Option<FullyQualifiedName>,
    /// Current method context for resolving `super`.
    pub(in crate::inference::type_tracker) method: Option<RubyMethod>,
}

#[derive(Default)]
pub(in crate::inference::type_tracker) struct AnalysisContext {
    /// Engine for method return type lookups on analysis path
    pub(in crate::inference::type_tracker) engine: Option<Arc<RwLock<AnalysisEngine>>>,
    pub(in crate::inference::type_tracker) query_cache: Option<Arc<AnalysisQueryCache>>,
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
    pub fn with_analysis_engine(mut self, analysis_engine: Arc<RwLock<AnalysisEngine>>) -> Self {
        self.analysis.engine = Some(analysis_engine);
        self
    }

    pub fn with_analysis_query_cache(mut self, cache: Arc<AnalysisQueryCache>) -> Self {
        self.analysis.query_cache = Some(cache);
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
}
