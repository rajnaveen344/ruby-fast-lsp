//! Embedded RBS method catalogs for completion and method-existence checks.

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rbs_parser::Loader;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::conversion::{rbs_type_to_class_name, rbs_type_to_ruby_type};
use super::RBS_LOADER;
use crate::core::RubyType;

#[derive(Default)]
struct RbsMethodNameCache {
    instance: HashMap<String, Arc<HashSet<String>>>,
    singleton: HashMap<String, Arc<HashSet<String>>>,
}

/// The embedded RBS environment is immutable after initialization. Cache only
/// declared owners, so this process-wide cache is bounded by the finite set of
/// embedded declarations rather than arbitrary source identifiers.
static RBS_METHOD_NAMES: Lazy<RwLock<RbsMethodNameCache>> =
    Lazy::new(|| RwLock::new(RbsMethodNameCache::default()));

/// Method info for completion
#[derive(Debug, Clone)]
pub struct RbsMethodInfo {
    pub name: String,
    pub return_type: Option<RubyType>,
    pub is_singleton: bool,
    pub params: Vec<String>,
}

/// Get all methods for a class from RBS definitions, including inherited methods
/// from the ancestor chain (superclass + included modules).
pub fn get_rbs_class_methods(class_name: &str, include_singleton: bool) -> Vec<RbsMethodInfo> {
    let loader = RBS_LOADER.read();
    let mut methods = Vec::new();
    let mut seen_methods = HashSet::new();
    let mut visited = HashSet::new();

    collect_rbs_methods_recursive(
        &loader,
        class_name,
        include_singleton,
        &mut methods,
        &mut seen_methods,
        &mut visited,
    );

    methods
}

pub fn rbs_class_method_exists(
    class_name: &str,
    method_name: &str,
    include_singleton: bool,
) -> bool {
    rbs_method_name_catalog(class_name, include_singleton).contains(method_name)
}

pub(super) fn rbs_method_name_catalog(
    class_name: &str,
    include_singleton: bool,
) -> Arc<HashSet<String>> {
    {
        let cache = RBS_METHOD_NAMES.read();
        let catalogs = if include_singleton {
            &cache.singleton
        } else {
            &cache.instance
        };
        if let Some(catalog) = catalogs.get(class_name) {
            return Arc::clone(catalog);
        }
    }

    // Hold the write lock through construction so concurrent project indexers
    // single-flight the first catalog build instead of duplicating the same
    // recursive RBS traversal.
    let mut cache = RBS_METHOD_NAMES.write();
    let catalogs = if include_singleton {
        &mut cache.singleton
    } else {
        &mut cache.instance
    };
    if let Some(catalog) = catalogs.get(class_name) {
        return Arc::clone(catalog);
    }

    let loader = RBS_LOADER.read();
    let owner_is_declared =
        loader.get_class(class_name).is_some() || loader.get_module(class_name).is_some();
    if !owner_is_declared {
        return Arc::new(HashSet::new());
    }

    let mut method_names = HashSet::new();
    let mut visited = HashSet::new();
    collect_rbs_method_names_recursive(
        &loader,
        class_name,
        include_singleton,
        &mut method_names,
        &mut visited,
    );

    let catalog = Arc::new(method_names);
    catalogs.insert(class_name.to_string(), Arc::clone(&catalog));
    catalog
}

/// Recursively collect methods from a class/module and its ancestors
fn collect_rbs_methods_recursive(
    loader: &Loader,
    class_name: &str,
    include_singleton: bool,
    methods: &mut Vec<RbsMethodInfo>,
    seen_methods: &mut std::collections::HashSet<String>,
    visited: &mut std::collections::HashSet<String>,
) {
    // Prevent infinite recursion from circular inheritance
    if !visited.insert(class_name.to_string()) {
        return;
    }

    // Collect methods from this class
    if let Some(class) = loader.get_class(class_name) {
        collect_methods_from_decl(&class.methods, include_singleton, methods, seen_methods);

        // Process aliases (e.g., `alias object_id __id__`)
        collect_aliases_from_members(
            &class.members,
            &class.methods,
            include_singleton,
            methods,
            seen_methods,
        );

        // Walk included modules (instance methods become available)
        for member in &class.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_methods_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        methods,
                        seen_methods,
                        visited,
                    );
                }
            }
        }

        // Walk superclass
        if let Some(superclass) = &class.superclass {
            if let Some(parent_name) = rbs_type_to_class_name(superclass) {
                collect_rbs_methods_recursive(
                    loader,
                    &parent_name,
                    include_singleton,
                    methods,
                    seen_methods,
                    visited,
                );
            }
        } else if class_name != "BasicObject" {
            // Implicit superclass is Object (unless we're BasicObject)
            collect_rbs_methods_recursive(
                loader,
                "Object",
                include_singleton,
                methods,
                seen_methods,
                visited,
            );
        }
    }

    // Also check modules (for when class_name is a module, or for mixed-in methods)
    if let Some(module) = loader.get_module(class_name) {
        collect_methods_from_decl(&module.methods, include_singleton, methods, seen_methods);

        // Process aliases in module
        collect_aliases_from_members(
            &module.members,
            &module.methods,
            include_singleton,
            methods,
            seen_methods,
        );

        // Walk included modules within this module
        for member in &module.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_methods_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        methods,
                        seen_methods,
                        visited,
                    );
                }
            }
        }
    }
}

fn collect_rbs_method_names_recursive(
    loader: &Loader,
    class_name: &str,
    include_singleton: bool,
    method_names: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(class_name.to_string()) {
        return;
    }

    if let Some(class) = loader.get_class(class_name) {
        collect_method_names_from_decl(&class.methods, include_singleton, method_names);
        collect_alias_names_from_members(&class.members, include_singleton, method_names);

        for member in &class.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_method_names_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        method_names,
                        visited,
                    );
                }
            }
        }

        if let Some(superclass) = &class.superclass {
            if let Some(parent_name) = rbs_type_to_class_name(superclass) {
                collect_rbs_method_names_recursive(
                    loader,
                    &parent_name,
                    include_singleton,
                    method_names,
                    visited,
                );
            }
        } else if class_name != "BasicObject" {
            collect_rbs_method_names_recursive(
                loader,
                "Object",
                include_singleton,
                method_names,
                visited,
            );
        }
    }

    if let Some(module) = loader.get_module(class_name) {
        collect_method_names_from_decl(&module.methods, include_singleton, method_names);
        collect_alias_names_from_members(&module.members, include_singleton, method_names);

        for member in &module.members {
            if let rbs_parser::Member::Include(module_type) = member {
                if let Some(module_name) = rbs_type_to_class_name(module_type) {
                    collect_rbs_method_names_recursive(
                        loader,
                        &module_name,
                        include_singleton,
                        method_names,
                        visited,
                    );
                }
            }
        }
    }
}

fn collect_method_names_from_decl(
    method_decls: &[rbs_parser::MethodDecl],
    include_singleton: bool,
    method_names: &mut HashSet<String>,
) {
    for method in method_decls {
        let is_singleton = method.kind == rbs_parser::MethodKind::Singleton;
        if !is_singleton || include_singleton {
            method_names.insert(method.name.clone());
        }
    }
}

fn collect_alias_names_from_members(
    members: &[rbs_parser::Member],
    include_singleton: bool,
    method_names: &mut HashSet<String>,
) {
    for member in members {
        if let rbs_parser::Member::Alias(alias) = member {
            if !alias.is_singleton || include_singleton {
                method_names.insert(alias.new_name.clone());
            }
        }
    }
}

/// Collect methods from a list of MethodDecl into the methods vec, skipping duplicates
fn collect_methods_from_decl(
    method_decls: &[rbs_parser::MethodDecl],
    include_singleton: bool,
    methods: &mut Vec<RbsMethodInfo>,
    seen_methods: &mut std::collections::HashSet<String>,
) {
    for method in method_decls {
        let is_singleton = method.kind == rbs_parser::MethodKind::Singleton;

        if is_singleton && !include_singleton {
            continue;
        }

        if !seen_methods.insert(method.name.clone()) {
            continue; // Already seen — subclass method takes priority
        }

        let params: Vec<String> = method
            .overloads
            .first()
            .map(|o| {
                o.params
                    .iter()
                    .map(|p| p.name.clone().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default();

        let return_type = method.return_type().map(rbs_type_to_ruby_type);

        methods.push(RbsMethodInfo {
            name: method.name.clone(),
            return_type,
            is_singleton,
            params,
        });
    }
}

/// Process alias declarations from class/module members.
/// For `alias object_id __id__`, creates a method entry for `object_id`
/// with the same signature as `__id__`.
fn collect_aliases_from_members(
    members: &[rbs_parser::Member],
    method_decls: &[rbs_parser::MethodDecl],
    include_singleton: bool,
    methods: &mut Vec<RbsMethodInfo>,
    seen_methods: &mut std::collections::HashSet<String>,
) {
    for member in members {
        if let rbs_parser::Member::Alias(alias) = member {
            if alias.is_singleton && !include_singleton {
                continue;
            }
            if !seen_methods.insert(alias.new_name.clone()) {
                continue;
            }

            // Look up the target method to copy its signature
            let target = method_decls.iter().find(|m| m.name == alias.old_name);
            let (return_type, params) = if let Some(target_method) = target {
                let rt = target_method.return_type().map(rbs_type_to_ruby_type);
                let ps: Vec<String> = target_method
                    .overloads
                    .first()
                    .map(|o| {
                        o.params
                            .iter()
                            .map(|p| p.name.clone().unwrap_or_default())
                            .collect()
                    })
                    .unwrap_or_default();
                (rt, ps)
            } else {
                (None, vec![])
            };

            methods.push(RbsMethodInfo {
                name: alias.new_name.clone(),
                return_type,
                is_singleton: alias.is_singleton,
                params,
            });
        }
    }
}
