//! Checks on calls that resolve to a method: runtime availability,
//! signature (arity and keyword) diagnostics, and arguments whose proven type
//! contradicts the parameter's declared type.

use std::collections::HashMap;

use log::debug;

use super::policy::{
    arity_mismatch, closest_keyword, AncestryCompleteness, MethodArity, ARGUMENT_TYPE_MISMATCH,
    MISSING_KWARG, UNKNOWN_KWARG, UNSUPPORTED_RUNTIME_API, WRONG_ARITY,
};
use crate::core::storage::reference_store::{MethodCallDiagnostics, MethodCallSignature};
use crate::core::{
    ArgumentCandidate, ArgumentForm, ArgumentSlot, DiagnosticFact, FullyQualifiedName,
    GraphNodeKind, MethodAvailability, MethodFact, MethodParamFact, MethodParamKind, NamespaceKind,
    RubyConstant, RubyMethod, RubyType, SourceFileId, TextRange, TypeInferenceOutcome,
    TypeProvenance, TypeSubject,
};
use crate::engine::lookup::MethodAnswer;
use crate::engine::resolution::method_lookup_chain;
use crate::engine::{Project, View};

/// Argument type checks of one resolve pass. A check waits until every call
/// of the pass has an outcome, because an argument that is itself a call is
/// visited after the call it is passed to.
#[derive(Default)]
pub(super) struct PendingArgumentChecks {
    checks: Vec<PendingArgumentCheck>,
}

struct PendingArgumentCheck {
    argument: ArgumentCandidate,
    declared: RubyType,
    /// Namespace the declaration was written in; its type names resolve here.
    declared_in: FullyQualifiedName,
    parameter: ustr::Ustr,
    method: RubyMethod,
}

/// Numeric classes Ruby converts between freely; a declared member of this
/// family accepts any other member.
const NUMERIC_FAMILY: [&str; 4] = ["Integer", "Float", "Rational", "Complex"];

impl Project {
    pub(super) fn push_unavailable_method_diagnostic(
        &self,
        fact: &MethodFact,
        method: &crate::core::RubyMethod,
        diagnostic_range: TextRange,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let MethodAvailability::Unavailable { reason } = &fact.availability else {
            return;
        };
        diagnostics_by_file
            .entry(diagnostic_range.file_id)
            .or_default()
            .push(UNSUPPORTED_RUNTIME_API.fact(
                diagnostic_range,
                format!(
                    "Runtime API `{}` is unavailable: {}",
                    method.as_str(),
                    reason
                ),
            ));
    }

    pub(super) fn push_signature_diagnostics(
        &self,
        fact: &MethodFact,
        requested_owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
        diagnostics: MethodCallDiagnostics<'_>,
        argument_checks: Option<&mut PendingArgumentChecks>,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let Some(signature) = diagnostics.signature() else {
            return;
        };
        let diagnostic_range = diagnostics.diagnostic_range();
        if !fact.has_complete_parameter_shape() {
            return;
        }

        let arity = MethodArity::from_params(&fact.param_facts);
        let declares_keywords = arity.has_kwrest
            || !arity.required_keywords.is_empty()
            || !arity.optional_keywords.is_empty();
        let keywords_form_options_hash = !declares_keywords && signature.has_nonempty_keyword_hash;
        let mut effective_signature = signature;
        if keywords_form_options_hash {
            effective_signature.positional_count += 1;
        }
        if !declares_keywords && signature.has_keyword_splat {
            // The splatted hash may be empty or non-empty. Under options-hash
            // calling semantics that makes the positional count a range, not
            // one additional proven argument.
            effective_signature.has_positional_splat = true;
        }
        let mismatch = if declares_keywords
            && effective_signature.trailing_positional_may_be_options_hash
        {
            invariant!(
                effective_signature.positional_count > 0,
                what = "a trailing positional options-hash marker exists without a positional argument",
                why = "the marker can only be set while counting the final positional argument",
                fix = "update positional_count and trailing_positional_may_be_options_hash atomically",
            );
            let direct_mismatch = arity_mismatch(&effective_signature, &arity);
            let mut converted_signature = effective_signature;
            converted_signature.positional_count -= 1;
            let converted_mismatch = arity_mismatch(&converted_signature, &arity);
            match (direct_mismatch, converted_mismatch) {
                (Some(direct), Some(_converted)) => Some(direct),
                (Some(_direct), None) => None,
                (None, Some(_converted)) => None,
                (None, None) => None,
            }
        } else {
            arity_mismatch(&effective_signature, &arity)
        };
        if let (None, Some(argument_checks)) = (&mismatch, argument_checks) {
            let keywords_bind_by_name =
                declares_keywords && !effective_signature.trailing_positional_may_be_options_hash;
            self.defer_argument_type_checks(
                fact,
                method,
                signature,
                keywords_bind_by_name,
                argument_checks,
            );
        }
        if let Some((min, max, actual)) = mismatch {
            if log::log_enabled!(
                target: "ruby_analysis::engine::diagnostics::wrong_arity",
                log::Level::Debug
            ) {
                let lookup_chain = method_lookup_chain(self, requested_owner);
                debug!(
                    target: "ruby_analysis::engine::diagnostics::wrong_arity",
                    "wrong-arity proof: method={} receiver={} requested_owner={} resolved_fqn={} owner={} definition_file_id={} definition_range={}..{} call_file_id={} call_range={}..{} positional={} params={:?} lookup_chain={:?}",
                    method.as_str(),
                    self.names
                        .method_receiver_label(diagnostics)
                        .as_deref()
                        .unwrap_or("<implicit-self>"),
                    requested_owner,
                    fact.fqn,
                    fact.owner,
                    fact.range.file_id.0,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    diagnostic_range.file_id.0,
                    diagnostic_range.start_byte,
                    diagnostic_range.end_byte,
                    actual,
                    fact.param_facts,
                    lookup_chain,
                );
            }
            let expected = match max {
                Some(max) if max == min => format!("{}", min),
                Some(max) => format!("{}..{}", min, max),
                None => format!("{}+", min),
            };
            diagnostics_by_file
                .entry(diagnostic_range.file_id)
                .or_default()
                .push(WRONG_ARITY.fact(
                    diagnostic_range,
                    format!(
                        "Wrong number of arguments for `{}` (expected {}, got {})",
                        method.as_str(),
                        expected,
                        actual
                    ),
                ));
        }

        if declares_keywords && !arity.has_kwrest && !signature.has_keyword_splat {
            let declared = arity
                .required_keywords
                .iter()
                .chain(arity.optional_keywords.iter())
                .cloned()
                .collect::<Vec<_>>();
            for kwarg in signature.keyword_args {
                if declared.iter().any(|name| name == kwarg.name.as_str()) {
                    continue;
                }
                let suggestion = closest_keyword(kwarg.name.as_str(), &declared);
                let mut message = format!(
                    "Unknown keyword argument `{}:` for `{}`",
                    kwarg.name,
                    method.as_str()
                );
                if let Some(suggestion) = suggestion {
                    message.push_str(&format!(". Did you mean `{}:`?", suggestion));
                }
                diagnostics_by_file
                    .entry(kwarg.range.file_id)
                    .or_default()
                    .push(UNKNOWN_KWARG.fact(kwarg.range, message));
            }
        }

        if !arity.required_keywords.is_empty()
            && !signature.has_keyword_splat
            && !signature.has_positional_splat
            && !signature.trailing_positional_may_be_options_hash
        {
            let supplied = signature
                .keyword_args
                .iter()
                .map(|kwarg| kwarg.name.as_str())
                .collect::<Vec<_>>();
            let mut missing = arity
                .required_keywords
                .iter()
                .filter(|kwarg| !supplied.contains(&kwarg.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            missing.sort();
            if !missing.is_empty() {
                let kw_list = missing
                    .iter()
                    .map(|kwarg| format!("`{}:`", kwarg))
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics_by_file
                    .entry(diagnostic_range.file_id)
                    .or_default()
                    .push(MISSING_KWARG.fact(
                        diagnostic_range,
                        format!(
                            "Missing required keyword argument(s) for `{}`: {}",
                            method.as_str(),
                            kw_list
                        ),
                    ));
            }
        }
    }
}

impl Project {
    /// Queue every argument whose parameter is known and has a declared type.
    /// `keywords_bind_by_name` is false when the call's keywords, or its last
    /// positional argument, may travel as one options hash.
    fn defer_argument_type_checks(
        &self,
        fact: &MethodFact,
        method: &RubyMethod,
        signature: MethodCallSignature<'_>,
        keywords_bind_by_name: bool,
        pending: &mut PendingArgumentChecks,
    ) {
        if signature.arguments.is_empty()
            || signature.has_positional_splat
            || signature.has_keyword_splat
        {
            return;
        }
        let declares_keywords = fact.param_facts.iter().any(|parameter| {
            matches!(
                parameter.kind,
                MethodParamKind::RequiredKeyword
                    | MethodParamKind::OptionalKeyword
                    | MethodParamKind::KeywordRest
                    | MethodParamKind::AnonymousKeywordRest
            )
        });
        if declares_keywords && !keywords_bind_by_name {
            return;
        }
        let positional = positional_parameters(&fact.param_facts);
        for argument in signature.arguments {
            let parameter = match argument.slot {
                ArgumentSlot::Positional(index) => positional
                    .as_ref()
                    .and_then(|parameters| parameters.get(usize::from(index)))
                    .map(|parameter| parameter.name),
                ArgumentSlot::Keyword(index) => signature
                    .keyword_args
                    .get(usize::from(index))
                    .filter(|_| keywords_bind_by_name)
                    .and_then(|keyword| {
                        fact.param_facts.iter().find(|parameter| {
                            parameter.name == keyword.name
                                && matches!(
                                    parameter.kind,
                                    MethodParamKind::RequiredKeyword
                                        | MethodParamKind::OptionalKeyword
                                )
                        })
                    })
                    .map(|parameter| parameter.name),
            };
            let Some(parameter) = parameter else {
                continue;
            };
            let Some(declared) = self.declared_parameter_type(fact, parameter) else {
                continue;
            };
            pending.checks.push(PendingArgumentCheck {
                argument: argument.clone(),
                declared,
                declared_in: fact.owner.clone(),
                parameter,
                method: *method,
            });
        }
    }

    /// The single type the method's documentation declares for one parameter.
    /// Inferred parameter types are evidence, not a contract, and never
    /// answer here.
    fn declared_parameter_type(
        &self,
        fact: &MethodFact,
        parameter: ustr::Ustr,
    ) -> Option<RubyType> {
        let subject = TypeSubject::Parameter {
            method: fact.fqn.clone(),
            name: parameter,
        };
        // Documented overloads give one parameter several types; only a
        // single agreed type is a contract this check can hold a call to.
        let mut documented = self
            .type_store()
            .facts_for(&subject)
            .into_iter()
            .filter(|stored| {
                stored.range.file_id == fact.range.file_id
                    && fact.range.start_byte <= stored.range.start_byte
                    && stored.range.end_byte <= fact.range.end_byte
                    && stored.provenance == TypeProvenance::Yard
            })
            .map(|stored| stored.ruby_type);
        let declared = match documented.next() {
            Some(first) => {
                if documented.any(|other| other != first) {
                    return None;
                }
                first
            }
            None => return None,
        };
        (declared != RubyType::Unknown).then_some(declared)
    }

    /// Report each queued argument whose proven type lies outside its
    /// parameter's declared type. `call_outcomes` holds the pass's calls.
    pub(super) fn push_argument_type_diagnostics(
        &self,
        pending: PendingArgumentChecks,
        call_outcomes: &HashMap<TextRange, TypeInferenceOutcome>,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        if pending.checks.is_empty() {
            return;
        }
        let mut completeness = AncestryCompleteness::new(self);
        for check in pending.checks {
            let range = check.argument.range;
            let value_is_fixed = !matches!(
                check.argument.form,
                ArgumentForm::Local(_) | ArgumentForm::Call
            );
            let argument_type = match check.argument.form {
                // The indexed type is the flow tracker's own proof.
                ArgumentForm::Local(ruby_type) => Some(*ruby_type),
                ArgumentForm::Call => self.proven_deferred_receiver_type(range, call_outcomes),
                ArgumentForm::String => Some(RubyType::string()),
                ArgumentForm::Symbol => Some(RubyType::symbol()),
                ArgumentForm::Integer => Some(RubyType::integer()),
                ArgumentForm::Float => Some(RubyType::float()),
                ArgumentForm::True => Some(RubyType::true_class()),
                ArgumentForm::False => Some(RubyType::false_class()),
                ArgumentForm::Array => Some(RubyType::class("Array")),
                ArgumentForm::Hash => Some(RubyType::class("Hash")),
                ArgumentForm::Range => Some(RubyType::class("Range")),
                ArgumentForm::Regexp => Some(RubyType::class("Regexp")),
            };
            let Some(argument_type) = argument_type.map(|ruby_type| ruby_type.widen_literals())
            else {
                continue;
            };
            if !self.argument_contradicts_declaration(
                &argument_type,
                &check.declared,
                &check.declared_in,
                value_is_fixed,
                &mut completeness,
            ) {
                continue;
            }
            diagnostics_by_file.entry(range.file_id).or_default().push(
                ARGUMENT_TYPE_MISMATCH.fact(
                    range,
                    format!(
                        "Argument `{}` of `{}` expects `{}`, got `{}`",
                        check.parameter,
                        check.method.as_str(),
                        check.declared,
                        argument_type
                    ),
                ),
            );
        }
    }

    /// Whether every member of `argument` is proven to lie outside every
    /// member of `declared`. Any member either side cannot model answers
    /// false: the contradiction is then unproven.
    fn argument_contradicts_declaration(
        &self,
        argument: &RubyType,
        declared: &RubyType,
        declared_in: &FullyQualifiedName,
        value_is_fixed: bool,
        completeness: &mut AncestryCompleteness,
    ) -> bool {
        let view = View::new(self);
        let mut declared_names = Vec::new();
        for member in union_members(declared) {
            let Some(name) = class_name_parts(member) else {
                return false;
            };
            // A declared name is written relative to the method's namespace.
            let scope = declared_in.namespace_parts();
            let Some(kind) = (0..=scope.len()).rev().find_map(|depth| {
                let mut parts = scope[..depth].to_vec();
                parts.extend(name.iter().cloned());
                view.namespace_node_kind(&FullyQualifiedName::namespace_with_kind(
                    parts,
                    NamespaceKind::Instance,
                ))
            }) else {
                return false;
            };
            // An object can be extended with a module after it is created, so
            // only a literal value proves a module is absent.
            if kind == GraphNodeKind::Module && !value_is_fixed {
                return false;
            }
            declared_names.push(name);
        }
        if declared_names.is_empty() {
            return false;
        }

        let method_missing = RubyMethod::new("method_missing").ok();
        for member in union_members(argument) {
            if !matches!(
                member,
                RubyType::Class(_) | RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Shape(_)
            ) {
                return false;
            }
            let Some(name) = class_name_parts(member) else {
                return false;
            };
            if name.len() == 1 && name[0].as_str() == "NilClass" {
                return false;
            }
            let class = FullyQualifiedName::namespace_with_kind(name, NamespaceKind::Instance);
            if view.namespace_node_kind(&class) != Some(GraphNodeKind::Class)
                || matches!(
                    completeness.chain(self, &class, |_| false),
                    MethodAnswer::Unknown(_)
                )
            {
                return false;
            }
            let ancestors = method_lookup_chain(self, &class)
                .into_iter()
                .map(|ancestor| ancestor.namespace_parts())
                .collect::<Vec<_>>();
            let in_numeric_family = |parts: &[RubyConstant]| {
                parts.len() == 1 && NUMERIC_FAMILY.contains(&parts[0].as_str())
            };
            let accepted = declared_names.iter().any(|declared| {
                ancestors
                    .iter()
                    .any(|ancestor| ancestor.ends_with(declared))
                    || (in_numeric_family(declared)
                        && ancestors.first().is_some_and(|own| in_numeric_family(own)))
            });
            if accepted {
                return false;
            }
            // A delegating class answers for the type it wraps without
            // inheriting from it.
            let delegates = ancestors.iter().any(|ancestor| {
                let is_root = ancestor.len() == 1
                    && matches!(ancestor[0].as_str(), "BasicObject" | "Object" | "Kernel");
                if is_root {
                    return false;
                }
                let owner = FullyQualifiedName::namespace_with_kind(
                    ancestor.clone(),
                    NamespaceKind::Instance,
                );
                (ancestor.len() == 1 && ancestor[0].as_str() == "Delegator")
                    || method_missing.as_ref().is_some_and(|method| {
                        !view
                            .method_facts_matching_owner_name(&owner, method)
                            .is_empty()
                    })
            });
            if delegates {
                return false;
            }
        }
        true
    }
}

/// Required and optional positional parameters in call order, or `None`
/// when an argument's position does not decide its parameter.
fn positional_parameters(parameters: &[MethodParamFact]) -> Option<Vec<&MethodParamFact>> {
    let mut positional = Vec::new();
    let mut seen_optional = false;
    for parameter in parameters {
        match parameter.kind {
            MethodParamKind::Required if seen_optional => return None,
            MethodParamKind::Required => positional.push(parameter),
            MethodParamKind::Optional => {
                seen_optional = true;
                positional.push(parameter);
            }
            MethodParamKind::Rest
            | MethodParamKind::AnonymousRest
            | MethodParamKind::Forwarding => return None,
            MethodParamKind::RequiredKeyword
            | MethodParamKind::OptionalKeyword
            | MethodParamKind::KeywordRest
            | MethodParamKind::AnonymousKeywordRest
            | MethodParamKind::Block => {}
        }
    }
    Some(positional)
}

fn union_members(ruby_type: &RubyType) -> Vec<&RubyType> {
    match ruby_type {
        RubyType::Union(members) => members.iter().flat_map(union_members).collect(),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Unknown => vec![ruby_type],
    }
}

/// The class or module name an instance type stands for. Element types of a
/// collection are not compared.
fn class_name_parts(ruby_type: &RubyType) -> Option<Vec<RubyConstant>> {
    let builtin = |name: &str| RubyConstant::new(name).ok().map(|constant| vec![constant]);
    match ruby_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => Some(fqn.namespace_parts()),
        RubyType::Array(_) => builtin("Array"),
        RubyType::Hash(_, _) | RubyType::Shape(_) => builtin("Hash"),
        RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}
