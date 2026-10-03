//! Method signature fact resolution and its query cache paths.

use super::receiver_type_members;
use crate::core::{FullyQualifiedName, MethodCalleeResolution, MethodFact, RubyMethod, RubyType};
use crate::engine::lookup::{self, LookupReceiver, MethodRequest, MethodWant};
use crate::engine::queries::View;
use crate::inference::semantics::ReceiverAccess;
use crate::invariant::ExpectInvariant;

impl<'a> View<'a> {
    /// Signature facts every member of `receiver_type` proves; empty when a
    /// member proves none. Each namespace is read through the lookup, so a
    /// memoized view serves it.
    pub(in crate::engine) fn resolve_method_signature_facts_for_type_inner(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        access: ReceiverAccess<'_>,
    ) -> Vec<MethodFact> {
        let members = receiver_type_members(receiver_type);
        let mut all_facts = Vec::new();
        for member in members {
            let namespaces = crate::core::receiver_type_to_method_namespaces(member);
            if namespaces.is_empty() {
                return Vec::new();
            }

            let mut member_facts = Vec::new();
            for namespace in namespaces {
                let request = MethodRequest {
                    receiver: LookupReceiver::Namespace(&namespace),
                    method: *method,
                    access,
                    want: MethodWant::Signatures,
                };
                member_facts.extend(
                    lookup::method(self, request)
                        .into_signatures()
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

    pub(in crate::engine) fn resolve_method_signature_facts_inner(
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
                    .method_facts_matching_owner_name(&callee.owner, method)
                    .into_iter()
                    .collect::<Vec<_>>();
                let signatures = matching
                    .iter()
                    .filter(|fact| {
                        self.file(fact.range.file_id)
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
