//! Java navigation: verified source and decompiled implementation selection,
//! generated signatures, and the static navigation plan for a Ruby source.

use super::{JavaImplementationResolutionError, JrubyImportProvider, StaticJavaNavigationPlan};
use crate::environment::runtime::jruby::source_navigation::{
    JavaSourceResolutionError, ResolvedJavaSource,
};
use ruby_analysis::core::{RubyMethod, SourceFileId, TextRange};
use ruby_fast_lsp_jruby_support::{static_navigation_scan, JavaClassName, StaticJavaDependency};
use ruby_fast_lsp_jvm_metadata::{JavaSourceClassLocation, MemberInfo, Visibility};
use ruby_prism::Node;
use std::collections::{BTreeMap, BTreeSet};

impl JrubyImportProvider {
    pub(crate) fn register_method_navigation_ranges(
        &self,
        internal_name: &str,
        location: &JavaSourceClassLocation,
        file_id: SourceFileId,
    ) {
        invariant_eq!(
            internal_name,
            location.internal_name,
            what = "JRuby navigation registration received mismatched class identities",
            why = "verified Java source locations belong to exactly one catalog class",
            fix = "register each location with the internal class name used to resolve it",
        );
        self.registered_navigation_classes
            .write()
            .insert(internal_name.to_string());
        let mut ranges = self.method_navigation_ranges.write();
        for method in &location.methods {
            let key = (
                internal_name.to_string(),
                method.name.clone(),
                method.descriptor.clone(),
            );
            let range = TextRange::new(
                file_id,
                method.declaration_range.start,
                method.declaration_range.end,
            );
            if let Some(previous) = ranges.insert(key.clone(), range) {
                invariant_eq!(
                    previous,
                    range,
                    what = "one JVM method identity mapped to two implementation ranges",
                    why = "source/decompiler verification must select one exact member",
                    fix = "reject ambiguous Java source before navigation registration",
                );
            }
        }
    }

    pub(super) fn preferred_method_definition_range(
        &self,
        internal_name: &str,
        method: &MemberInfo,
    ) -> Option<TextRange> {
        self.method_navigation_ranges
            .read()
            .get(&(
                internal_name.to_string(),
                method.name.clone(),
                method.descriptor.clone(),
            ))
            .copied()
    }

    pub(crate) fn has_registered_navigation_class(&self, internal_name: &str) -> bool {
        self.registered_navigation_classes
            .read()
            .contains(internal_name)
    }

    pub fn resolved_source(
        &self,
        internal_name: &str,
    ) -> Result<Option<ResolvedJavaSource>, JavaSourceResolutionError> {
        let Some(resolver) = &self.source_resolver else {
            return Ok(None);
        };
        let Some(declaration) = self.catalog.classes.get(internal_name) else {
            return Ok(None);
        };
        resolver.resolve(declaration)
    }

    pub fn resolved_navigation_implementations(
        &self,
        internal_name: &str,
    ) -> Result<Vec<ResolvedJavaSource>, JavaImplementationResolutionError> {
        let Some(declaration) = self.catalog.classes.get(internal_name) else {
            return Ok(Vec::new());
        };
        let exact_source = self
            .resolved_source(internal_name)
            .map_err(JavaImplementationResolutionError::Source)?;
        let Some(decompiler) = &self.decompiler else {
            return Ok(exact_source.into_iter().collect());
        };

        if let Some(exact_source) = exact_source {
            if !has_missing_concrete_navigation_methods(&declaration.class, &exact_source.location)
            {
                return Ok(vec![exact_source]);
            }
            let Some(mut decompiled) = decompiler
                .decompile(declaration)
                .map_err(JavaImplementationResolutionError::Decompiler)?
            else {
                return Ok(vec![exact_source]);
            };
            let Some(mut supplemental) =
                supplemental_implementation_location(&exact_source.location, decompiled.location)
            else {
                return Ok(vec![exact_source]);
            };
            supplemental.methods.retain(|location| {
                declaration.class.methods.iter().any(|method| {
                    method.name == location.name
                        && method.descriptor == location.descriptor
                        && concrete_navigation_method(method)
                })
            });
            if supplemental.methods.is_empty() {
                return Ok(vec![exact_source]);
            }
            supplemental.fields.clear();
            decompiled.location = supplemental;
            return Ok(vec![exact_source, decompiled]);
        }

        Ok(decompiler
            .decompile(declaration)
            .map_err(JavaImplementationResolutionError::Decompiler)?
            .into_iter()
            .collect())
    }

    pub fn generated_signature(
        &self,
        import_name: &str,
    ) -> Result<Option<(String, String)>, ruby_fast_lsp_jruby_support::SignatureError> {
        let Ok(java_name) = JavaClassName::parse(import_name) else {
            return Ok(None);
        };
        let Some(declaration) = self.catalog.classes.get(java_name.internal_name()) else {
            return Ok(None);
        };
        let source = ruby_fast_lsp_jruby_support::generate_ruby_signature(&declaration.class)?;
        Ok(Some((java_name.internal_name().to_string(), source)))
    }

    pub fn static_navigation_plan(&self, source: &str) -> Result<StaticJavaNavigationPlan, String> {
        let parse = ruby_prism::parse(source.as_bytes());
        self.static_navigation_plan_for_node(&parse.node())
    }

    pub fn static_navigation_plan_for_node(
        &self,
        node: &Node<'_>,
    ) -> Result<StaticJavaNavigationPlan, String> {
        let mut signature_class_names = BTreeSet::new();
        let mut implementation_class_names = BTreeSet::new();
        let scan = static_navigation_scan(node);
        for dependency in scan.dependencies {
            match dependency {
                StaticJavaDependency::Class(name) => {
                    if let Some(class_name) = self.class_name_for_static_proxy_reference(&name)? {
                        signature_class_names.insert(class_name.clone());
                        implementation_class_names.insert(class_name);
                    }
                }
                StaticJavaDependency::Package(package) => {
                    signature_class_names.extend(self.class_names_in_package(&package)?);
                }
            }
        }
        for reference in scan.proxy_references {
            if let Some(class_name) = self.class_name_for_static_proxy_reference(&reference)? {
                signature_class_names.insert(class_name.clone());
                implementation_class_names.insert(class_name);
            }
        }
        let mut package_classes_by_constant = BTreeMap::<String, Vec<String>>::new();
        for internal_name in &signature_class_names {
            let Ok(name) = JavaClassName::parse(internal_name) else {
                continue;
            };
            package_classes_by_constant
                .entry(name.imported_constant().to_string())
                .or_default()
                .push(internal_name.clone());
        }
        for constant in scan.constant_references {
            let Some(candidates) = package_classes_by_constant.get(&constant) else {
                continue;
            };
            if candidates.len() == 1 {
                implementation_class_names.insert(candidates[0].clone());
            }
        }
        Ok(StaticJavaNavigationPlan {
            signature_class_names: signature_class_names.into_iter().collect(),
            implementation_class_names: implementation_class_names.into_iter().collect(),
        })
    }
}

fn has_missing_concrete_navigation_methods(
    class: &ruby_fast_lsp_jvm_metadata::ClassFile,
    exact: &JavaSourceClassLocation,
) -> bool {
    class.methods.iter().any(|method| {
        concrete_navigation_method(method)
            && !exact.methods.iter().any(|location| {
                location.name == method.name && location.descriptor == method.descriptor
            })
    })
}

fn concrete_navigation_method(method: &MemberInfo) -> bool {
    !method.is_abstract()
        && !method.is_native()
        && method.name != "<clinit>"
        && (method.name == "<init>" || RubyMethod::new(&method.name).is_ok())
        && matches!(
            method.visibility(),
            Visibility::Public | Visibility::Protected
        )
}

pub(super) fn supplemental_implementation_location(
    exact: &JavaSourceClassLocation,
    mut decompiled: JavaSourceClassLocation,
) -> Option<JavaSourceClassLocation> {
    invariant_eq!(
        exact.internal_name,
        decompiled.internal_name,
        what = "exact and decompiled Java locations identify different classes",
        why = "per-member precedence can compare only one winning class identity",
        fix = "decompile the same catalog declaration selected by exact-source resolution",
    );
    decompiled.methods.retain(|candidate| {
        !exact.methods.iter().any(|preferred| {
            preferred.name == candidate.name && preferred.descriptor == candidate.descriptor
        })
    });
    decompiled.fields.retain(|candidate| {
        !exact.fields.iter().any(|preferred| {
            preferred.name == candidate.name && preferred.descriptor == candidate.descriptor
        })
    });
    (!decompiled.methods.is_empty() || !decompiled.fields.is_empty()).then_some(decompiled)
}
