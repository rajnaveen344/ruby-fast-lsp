use ruby_analysis_core::{TypeResolution, TypeSubject};
use ruby_analysis_engine::AnalysisQuery;

use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::rbs::{
    get_rbs_method_return_type_as_ruby_type, get_rbs_method_return_type_with_type_args,
};
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;

use super::IndexVisitor;

impl IndexVisitor {
    pub(super) fn resolve_method_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        if *receiver_type == RubyType::Unknown {
            return None;
        }

        if let RubyType::Union(types) = receiver_type {
            let mut return_types = Vec::new();
            for ty in types {
                if let Some(return_type) = self.resolve_method_return_type(ty, method_name) {
                    if !return_types.contains(&return_type) {
                        return_types.push(return_type);
                    }
                }
            }
            return match return_types.len() {
                0 => None,
                1 => return_types.pop(),
                2.. => Some(RubyType::union(return_types)),
            };
        }

        if method_name == "new" {
            if let RubyType::ClassReference(fqn) = receiver_type {
                return Some(RubyType::Class(fqn.clone()));
            }
        }

        self.resolve_method_return_type_from_analysis(receiver_type, method_name)
            .or_else(|| resolve_rbs_method_return_type(receiver_type, method_name))
    }

    fn resolve_method_return_type_from_analysis(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let method = RubyMethod::new(method_name).ok()?;
        let namespace = receiver_namespace_for_analysis(receiver_type)?;
        let analysis_engine = self.analysis_engine.as_ref()?;
        let engine = analysis_engine.lock();
        let query = AnalysisQuery::new(&engine);
        let callees = query.resolve_method_callees(&namespace, &method)?;

        let mut return_types = Vec::new();
        for callee in callees {
            if callee.definition_ranges.is_empty() {
                continue;
            }

            let method_fqn = FullyQualifiedName::method(callee.owner.namespace_parts(), method);
            if let Some(return_type) = self.local_method_return_type(&method_fqn) {
                if !return_types.contains(&return_type) {
                    return_types.push(return_type);
                }
                continue;
            }

            if let Some(return_type) = query.method_return_type_for_receiver(&callee.owner, &method)
            {
                if !return_types.contains(&return_type) {
                    return_types.push(return_type);
                }
            }
        }

        match return_types.len() {
            0 => None,
            1 => return_types.pop(),
            2.. => Some(RubyType::union(return_types)),
        }
    }

    fn local_method_return_type(&self, method_fqn: &FullyQualifiedName) -> Option<RubyType> {
        match self.type_store.type_at(
            &TypeSubject::MethodReturn(method_fqn.clone()),
            self.document.analysis_file_id(),
            u32::MAX,
        ) {
            TypeResolution::Resolved(fact) if fact.ruby_type != RubyType::Unknown => {
                Some(fact.ruby_type)
            }
            TypeResolution::Resolved(_)
            | TypeResolution::Ambiguous(_)
            | TypeResolution::Unresolved => None,
        }
    }
}

fn receiver_namespace_for_analysis(receiver_type: &RubyType) -> Option<FullyQualifiedName> {
    match receiver_type {
        RubyType::Class(fqn) | RubyType::Module(fqn) => fqn.to_instance_namespace(),
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            fqn.to_singleton_namespace()
        }
        RubyType::Array(_) => builtin_namespace("Array"),
        RubyType::Hash(_, _) => builtin_namespace("Hash"),
        RubyType::Union(_) => None,
        RubyType::Unknown => None,
    }
}

fn builtin_namespace(name: &str) -> Option<FullyQualifiedName> {
    let constant = RubyConstant::new(name).ok()?;
    Some(FullyQualifiedName::namespace(vec![constant]))
}

fn resolve_rbs_method_return_type(receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
    let class_name = rbs_class_name(receiver_type)?;
    let is_singleton = matches!(
        receiver_type,
        RubyType::ClassReference(_) | RubyType::ModuleReference(_)
    );
    let type_args = type_args_for_receiver(receiver_type);
    if type_args.is_empty() {
        get_rbs_method_return_type_as_ruby_type(&class_name, method_name, is_singleton)
    } else {
        get_rbs_method_return_type_with_type_args(
            &class_name,
            method_name,
            is_singleton,
            &type_args,
        )
    }
}

fn rbs_class_name(receiver_type: &RubyType) -> Option<String> {
    match receiver_type {
        RubyType::Class(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::Module(fqn)
        | RubyType::ModuleReference(fqn) => fqn.namespace_parts().last().map(ToString::to_string),
        RubyType::Array(_) => Some("Array".to_string()),
        RubyType::Hash(_, _) => Some("Hash".to_string()),
        RubyType::Union(_) => None,
        RubyType::Unknown => None,
    }
}

fn type_args_for_receiver(receiver_type: &RubyType) -> Vec<RubyType> {
    match receiver_type {
        RubyType::Array(element_types) => match element_types.len() {
            0 => Vec::new(),
            1 => vec![element_types[0].clone()],
            2.. => vec![RubyType::union(element_types.clone())],
        },
        RubyType::Hash(key_types, value_types) => {
            let key = match key_types.len() {
                0 => RubyType::Unknown,
                1 => key_types[0].clone(),
                2.. => RubyType::union(key_types.clone()),
            };
            let value = match value_types.len() {
                0 => RubyType::Unknown,
                1 => value_types[0].clone(),
                2.. => RubyType::union(value_types.clone()),
            };
            vec![key, value]
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Union(_)
        | RubyType::Unknown => Vec::new(),
    }
}
