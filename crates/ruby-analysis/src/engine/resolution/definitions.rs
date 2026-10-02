//! Constant resolution in context and definition ranges for symbols.

use crate::core::{FullyQualifiedName, RubyConstant, SymbolKind, TextRange, TypeSubject};
use crate::engine::queries::View;

impl<'a> View<'a> {
    pub fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> FullyQualifiedName {
        let current_fqn = FullyQualifiedName::namespace_with_kind(
            current_namespace.to_vec(),
            crate::core::NamespaceKind::Instance,
        );
        let resolved = resolve_constant_fqn(self.engine, path, false, &current_fqn)
            .unwrap_or_else(|| FullyQualifiedName::constant(path.to_vec()));
        let resolved_constant = FullyQualifiedName::constant(resolved.namespace_parts().to_vec());
        if let Some(receiver_type) = self.constant_value_type(&resolved_constant) {
            if let Some(namespace) = self.type_to_namespace(&receiver_type) {
                return namespace;
            }
        }

        FullyQualifiedName::namespace_with_kind(
            resolved.namespace_parts(),
            crate::core::NamespaceKind::Singleton,
        )
    }

    pub fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let context_fqn = FullyQualifiedName::namespace(context.to_vec());
        resolve_constant_fqn(self.engine, parts, false, &context_fqn)
    }
}

impl<'a> View<'a> {
    pub fn constant_definition_ranges(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Vec<TextRange> {
        let fqn = self
            .resolve_constant_in_context(parts, context)
            .unwrap_or_else(|| FullyQualifiedName::constant(parts.to_vec()));
        let mut runtime_targets = self
            .type_facts_for(&TypeSubject::Constant(fqn.clone()))
            .into_iter()
            .filter(|fact| fact.provenance == crate::core::TypeProvenance::Runtime)
            .filter_map(|fact| match fact.ruby_type {
                crate::core::RubyType::ClassReference(target)
                | crate::core::RubyType::ModuleReference(target)
                    if target != fqn =>
                {
                    Some(target)
                }
                crate::core::RubyType::ClassReference(_)
                | crate::core::RubyType::ModuleReference(_)
                | crate::core::RubyType::Class(_)
                | crate::core::RubyType::Module(_)
                | crate::core::RubyType::Array(_)
                | crate::core::RubyType::Hash(_, _)
                | crate::core::RubyType::Literal(_)
                | crate::core::RubyType::Shape(_)
                | crate::core::RubyType::Union(_)
                | crate::core::RubyType::Unknown => None,
            })
            .collect::<Vec<_>>();
        runtime_targets.sort_by_key(ToString::to_string);
        runtime_targets.dedup();
        if runtime_targets.len() == 1 {
            let implementation_ranges = self.symbol_definition_ranges(
                &runtime_targets[0],
                &[SymbolKind::Class, SymbolKind::Module, SymbolKind::Constant],
            );
            if !implementation_ranges.is_empty() {
                return implementation_ranges;
            }
        }
        self.symbol_definition_ranges(
            &fqn,
            &[SymbolKind::Class, SymbolKind::Module, SymbolKind::Constant],
        )
    }

    pub fn yard_type_definition_ranges(
        &self,
        type_name: &str,
        context: &[RubyConstant],
    ) -> Vec<TextRange> {
        let builtins = ["nil", "true", "false", "void", "Boolean", "bool"];
        if builtins
            .iter()
            .any(|builtin| builtin.eq_ignore_ascii_case(type_name))
        {
            return Vec::new();
        }

        let is_root_constant = type_name.starts_with("::");
        let type_to_parse = if is_root_constant {
            &type_name[2..]
        } else {
            type_name
        };

        let mut parts = Vec::new();
        for part in type_to_parse.split("::") {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(constant) = RubyConstant::try_from(trimmed) else {
                return Vec::new();
            };
            parts.push(constant);
        }

        if parts.is_empty() {
            return Vec::new();
        }

        let context = if is_root_constant { &[][..] } else { context };
        self.constant_definition_ranges(&parts, context)
    }

    pub fn variable_definition_ranges(&self, fqn: &FullyQualifiedName) -> Vec<TextRange> {
        self.symbol_definition_ranges(
            fqn,
            &[
                SymbolKind::LocalVariable,
                SymbolKind::InstanceVariable,
                SymbolKind::ClassVariable,
                SymbolKind::GlobalVariable,
            ],
        )
    }

    pub fn instance_variable_definition_ranges(&self, name: &str) -> Vec<TextRange> {
        match FullyQualifiedName::instance_variable(name.to_string()) {
            Ok(fqn) => self.variable_definition_ranges(&fqn),
            Err(_) => Vec::new(),
        }
    }

    pub fn class_variable_definition_ranges(&self, name: &str) -> Vec<TextRange> {
        match FullyQualifiedName::class_variable(name.to_string()) {
            Ok(fqn) => self.variable_definition_ranges(&fqn),
            Err(_) => Vec::new(),
        }
    }

    pub fn global_variable_definition_ranges(&self, name: &str) -> Vec<TextRange> {
        match FullyQualifiedName::global_variable(name.to_string()) {
            Ok(fqn) => self.variable_definition_ranges(&fqn),
            Err(_) => Vec::new(),
        }
    }
}

impl<'a> View<'a> {
    pub fn symbol_definition_ranges(
        &self,
        fqn: &FullyQualifiedName,
        allowed_kinds: &[SymbolKind],
    ) -> Vec<TextRange> {
        let ranges = self
            .symbol_facts_for(fqn)
            .into_iter()
            .filter(|fact| allowed_kinds.contains(&fact.kind))
            .map(|fact| fact.range)
            .collect();
        self.preferred_definition_ranges(ranges)
    }

    pub(super) fn resolve_constant_reference_target(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        self.engine
            .resolve_constant_reference(parts, current_namespace)
    }
}

fn resolve_constant_fqn(
    engine: &crate::engine::Project,
    parts: &[RubyConstant],
    absolute: bool,
    context_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let current_namespace = if absolute {
        &[][..]
    } else {
        context_fqn.namespace_parts_slice()
    };
    engine.resolve_constant_path(parts, current_namespace, false, true)
}
