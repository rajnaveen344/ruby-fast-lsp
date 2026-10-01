//! Method signature fact resolution and its query cache paths.

use super::receiver_type_members;
use crate::core::{FullyQualifiedName, MethodCalleeResolution, MethodFact, RubyMethod, RubyType};
use crate::engine::queries::cache::{AnalysisQueryCache, MethodReturnQueryAccess};
use crate::engine::queries::AnalysisQuery;
use crate::invariant::ExpectInvariant;

impl<'a> AnalysisQuery<'a> {
    pub fn resolve_method_signature_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_inner(namespace_fqn, method, true, None)
    }

    pub fn resolve_method_signature_facts_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        cache: &AnalysisQueryCache,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_cached_arc(namespace_fqn, method, cache)
            .as_ref()
            .clone()
    }

    pub(crate) fn resolve_method_signature_facts_cached_arc(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        cache: &AnalysisQueryCache,
    ) -> std::sync::Arc<Vec<MethodFact>> {
        self.resolve_method_signature_facts_maybe_cached(
            namespace_fqn,
            method,
            true,
            None,
            Some(cache),
        )
    }

    pub fn resolve_protected_method_signature_facts(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_inner(
            namespace_fqn,
            method,
            false,
            Some(caller_namespace_fqn),
        )
    }

    pub fn resolve_protected_method_signature_facts_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_for_type_inner(
            receiver_type,
            method,
            false,
            Some(caller_namespace_fqn),
            None,
        )
    }

    pub fn resolve_method_signature_facts_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_for_type_inner(receiver_type, method, true, None, None)
    }

    pub fn resolve_method_signature_facts_for_type_cached(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        cache: &AnalysisQueryCache,
    ) -> Vec<MethodFact> {
        self.resolve_method_signature_facts_for_type_inner(
            receiver_type,
            method,
            true,
            None,
            Some(cache),
        )
    }

    fn resolve_method_signature_facts_for_type_inner(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
        cache: Option<&AnalysisQueryCache>,
    ) -> Vec<MethodFact> {
        let members = receiver_type_members(receiver_type);
        let mut all_facts = Vec::new();
        for member in members {
            let namespaces = Self::receiver_type_to_method_namespaces(member);
            if namespaces.is_empty() {
                return Vec::new();
            }

            let mut member_facts = Vec::new();
            for namespace in namespaces {
                member_facts.extend(
                    self.resolve_method_signature_facts_maybe_cached(
                        &namespace,
                        method,
                        allow_private,
                        protected_caller,
                        cache,
                    )
                    .iter()
                    .cloned(),
                );
            }
            if member_facts.is_empty() {
                return Vec::new();
            }
            all_facts.extend(member_facts);
        }

        all_facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.fqn.to_string(),
            )
        });
        all_facts.dedup();
        all_facts
    }

    fn resolve_method_signature_facts_maybe_cached(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
        cache: Option<&AnalysisQueryCache>,
    ) -> std::sync::Arc<Vec<MethodFact>> {
        let Some(cache) = cache else {
            return std::sync::Arc::new(self.resolve_method_signature_facts_inner(
                namespace_fqn,
                method,
                allow_private,
                protected_caller,
            ));
        };
        let access = if let Some(caller) = protected_caller {
            MethodReturnQueryAccess::Protected(caller.clone())
        } else if allow_private {
            MethodReturnQueryAccess::Private
        } else {
            MethodReturnQueryAccess::Public
        };
        cache.method_signature_facts(
            self.engine.query_cache_identity(),
            namespace_fqn,
            *method,
            access,
            || {
                self.resolve_method_signature_facts_inner(
                    namespace_fqn,
                    method,
                    allow_private,
                    protected_caller,
                )
            },
        )
    }

    fn resolve_method_signature_facts_inner(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Vec<MethodFact> {
        let Some(callees) = self.resolve_method_callees_inner(
            namespace_fqn,
            method,
            allow_private,
            protected_caller,
            None,
        ) else {
            return Vec::new();
        };

        let mut facts = callees
            .into_iter()
            .filter(|callee| callee.resolution == MethodCalleeResolution::Exact)
            .flat_map(|callee| {
                let matching = self
                    .engine
                    .method_facts_matching_owner_name(&callee.owner, method)
                    .into_iter()
                    .collect::<Vec<_>>();
                let signatures = matching
                    .iter()
                    .filter(|fact| {
                        self.engine
                            .file(fact.range.file_id)
                            .expect_invariant(
                                "signature fact references an unregistered file",
                                "signature selection requires source metadata",
                                "replace signature facts only after registering their source file",
                            )
                            .kind
                            == crate::core::SourceKind::Signature
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if signatures.is_empty() {
                    matching
                        .into_iter()
                        .filter(|fact| callee.definition_ranges.contains(&fact.range))
                        .collect::<Vec<_>>()
                } else {
                    signatures
                }
            })
            .collect::<Vec<_>>();
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                fact.fqn.to_string(),
            )
        });
        facts.dedup();
        facts
    }
}
