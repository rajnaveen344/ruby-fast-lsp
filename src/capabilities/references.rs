use log::info;
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::analyzer_prism::{Identifier, MethodReceiver};
use crate::indexer::ancestor_chain::get_ancestor_chain;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MethodKind;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;

/// Find all references to a symbol at the given position.
pub async fn find_references_at_position(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    // Get the document content from the server
    let doc = match server.get_doc(uri) {
        Some(doc) => doc,
        None => {
            info!("Document not found for URI: {}", uri);
            return None;
        }
    };
    let content = doc.content.clone();

    // Use the analyzer to find the identifier at the position and get its fully qualified name
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
    let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

    identifier_opt.as_ref()?;

    let identifier = identifier_opt.unwrap();

    let index = server.index.lock();

    match &identifier {
        Identifier::RubyConstant { namespace: _, iden } => {
            // For namespaces, combine ancestors with the namespace parts
            let mut combined_ns = ancestors.clone();
            combined_ns.extend(iden.clone());
            let fqn = FullyQualifiedName::namespace(combined_ns);
            find_constant_references(&fqn, &index)
        }
        Identifier::RubyMethod {
            namespace,
            receiver,
            iden,
        } => find_method_references(namespace, receiver, iden, &index, &ancestors),
        Identifier::RubyLocalVariable { name, scope, .. } => {
            let fqn = FullyQualifiedName::local_variable(name.clone(), scope.clone()).unwrap();
            find_local_variable_references(&fqn, &index, uri, position)
        }
        Identifier::RubyInstanceVariable { name, .. } => {
            let fqn = FullyQualifiedName::instance_variable(name.clone()).unwrap();
            find_non_local_variable_references(&fqn, &index)
        }
        Identifier::RubyClassVariable { name, .. } => {
            let fqn = FullyQualifiedName::class_variable(name.clone()).unwrap();
            find_non_local_variable_references(&fqn, &index)
        }
        Identifier::RubyGlobalVariable { name, .. } => {
            let fqn = FullyQualifiedName::global_variable(name.clone()).unwrap();
            find_non_local_variable_references(&fqn, &index)
        }
        Identifier::YardType { type_name, .. } => {
            // For YARD types, find references to the class/module
            if let Some(fqn) =
                crate::yard::YardTypeConverter::parse_type_name_to_fqn_public(type_name)
            {
                find_constant_references(&fqn, &index)
            } else {
                None
            }
        }
    }
}

/// Find references to a constant
fn find_constant_references(
    fqn: &FullyQualifiedName,
    index: &crate::indexer::index::RubyIndex,
) -> Option<Vec<Location>> {
    if let Some(entries) = index.references.get(fqn) {
        if !entries.is_empty() {
            info!("Found {} constant references to: {}", entries.len(), fqn);
            return Some(entries.clone());
        }
    }

    info!("No constant references found for {}", fqn);
    None
}

/// Find references to a local variable with position filtering
fn find_local_variable_references(
    fqn: &FullyQualifiedName,
    index: &crate::indexer::index::RubyIndex,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    if let Some(entries) = index.references.get(fqn) {
        if !entries.is_empty() {
            let filtered_entries: Vec<Location> = entries
                .iter()
                .filter(|loc| loc.uri == *uri && loc.range.start >= position)
                .cloned()
                .collect();

            if !filtered_entries.is_empty() {
                info!(
                    "Found {} local variable references to: {}",
                    filtered_entries.len(),
                    fqn
                );
                return Some(filtered_entries);
            }
        }
    }

    info!("No local variable references found for {}", fqn);
    None
}

/// Find references to non-local variables (instance, class, global)
fn find_non_local_variable_references(
    fqn: &FullyQualifiedName,
    index: &crate::indexer::index::RubyIndex,
) -> Option<Vec<Location>> {
    if let Some(entries) = index.references.get(fqn) {
        if !entries.is_empty() {
            info!("Found {} variable references to: {}", entries.len(), fqn);
            return Some(entries.clone());
        }
    }

    info!("No variable references found for {}", fqn);
    None
}

/// Find references to a method, including mixin-aware references
fn find_method_references(
    _namespace: &[crate::types::ruby_namespace::RubyConstant],
    receiver: &MethodReceiver,
    method: &crate::types::ruby_method::RubyMethod,
    index: &crate::indexer::index::RubyIndex,
    ancestors: &[crate::types::ruby_namespace::RubyConstant],
) -> Option<Vec<Location>> {
    let mut all_references = Vec::new();

    match receiver {
        MethodReceiver::Constant(receiver_ns) => {
            // For constant receivers, we need to resolve the receiver namespace
            // For Platform::PlatformServices from GoshPosh::Platform::SpecHelpers,
            // this should resolve to GoshPosh::Platform::PlatformServices
            let receiver_fqn = if !receiver_ns.is_empty() && !ancestors.is_empty() {
                // Try to resolve relative to the current namespace
                // Look for the first part of receiver_ns in ancestors
                let first_receiver_part = &receiver_ns[0];

                // Find where this constant appears in ancestors
                if let Some(pos) = ancestors.iter().position(|c| c == first_receiver_part) {
                    // Build FQN up to that position + the receiver namespace
                    let mut resolved_ns = ancestors[..=pos].to_vec();
                    resolved_ns.extend(receiver_ns[1..].iter().cloned());
                    FullyQualifiedName::Constant(resolved_ns)
                } else {
                    // Not found in ancestors, treat as absolute from root
                    let mut full_ns = vec![ancestors[0].clone()];
                    full_ns.extend(receiver_ns.clone());
                    FullyQualifiedName::Constant(full_ns)
                }
            } else {
                // Simple case: prepend ancestors
                let mut full_ns = ancestors.to_vec();
                full_ns.extend(receiver_ns.clone());
                FullyQualifiedName::Constant(full_ns)
            };

            if let Some(refs) = find_method_references_with_receiver(&receiver_fqn, method, index) {
                all_references.extend(refs);
            }
        }
        MethodReceiver::None | MethodReceiver::SelfReceiver => {
            // No receiver, search in current context and mixins
            let receiver_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
            if let Some(refs) =
                find_method_references_without_receiver(&receiver_fqn, method, index)
            {
                all_references.extend(refs);
            }
        }
        MethodReceiver::LocalVariable(_)
        | MethodReceiver::InstanceVariable(_)
        | MethodReceiver::ClassVariable(_)
        | MethodReceiver::GlobalVariable(_)
        | MethodReceiver::MethodCall { .. }
        | MethodReceiver::Expression => {
            // Variable, method call, or expression receiver - search by method name across all contexts
            if let Some(refs) = find_method_references_by_name(method, index) {
                all_references.extend(refs);
            }
        }
    }

    if all_references.is_empty() {
        info!("No method references found for {:?}", method);
        None
    } else {
        info!(
            "Found {} method references to: {:?}",
            all_references.len(),
            method
        );
        Some(all_references)
    }
}

/// Find method references when called with a specific receiver
fn find_method_references_with_receiver(
    receiver_fqn: &FullyQualifiedName,
    method: &crate::types::ruby_method::RubyMethod,
    index: &crate::indexer::index::RubyIndex,
) -> Option<Vec<Location>> {
    let mut all_references = Vec::new();

    // Determine which method kinds to search for
    let kinds_to_check = if method.get_kind() == MethodKind::Unknown {
        vec![MethodKind::Instance, MethodKind::Class]
    } else {
        vec![method.get_kind()]
    };

    for kind in kinds_to_check {
        // Search in the receiver's ancestor chain
        if let Some(refs) =
            find_method_references_in_ancestor_chain(receiver_fqn, method, index, kind)
        {
            all_references.extend(refs);
        }
    }

    if all_references.is_empty() {
        None
    } else {
        Some(all_references)
    }
}

/// Find method references when called without a receiver
fn find_method_references_without_receiver(
    context_fqn: &FullyQualifiedName,
    method: &crate::types::ruby_method::RubyMethod,
    index: &crate::indexer::index::RubyIndex,
) -> Option<Vec<Location>> {
    let mut all_references = Vec::new();
    let method_kind = method.get_kind();

    // Search in current context and its ancestor chain
    if let Some(refs) =
        find_method_references_in_ancestor_chain(context_fqn, method, index, method_kind)
    {
        all_references.extend(refs);
    }

    // If we're in a module, also search in classes that include this module
    if let Some(refs) =
        find_method_references_in_sibling_modules(context_fqn, method, index, method_kind)
    {
        all_references.extend(refs);
    }

    if all_references.is_empty() {
        None
    } else {
        Some(all_references)
    }
}

/// Find method references by searching the ancestor chain
fn find_method_references_in_ancestor_chain(
    context_fqn: &FullyQualifiedName,
    method: &crate::types::ruby_method::RubyMethod,
    index: &crate::indexer::index::RubyIndex,
    kind: MethodKind,
) -> Option<Vec<Location>> {
    let mut all_references = Vec::new();
    let is_class_method = kind == MethodKind::Class;
    let ancestor_chain = get_ancestor_chain(index, context_fqn, is_class_method);

    for ancestor_fqn in ancestor_chain {
        let method_fqn = FullyQualifiedName::method(ancestor_fqn.namespace_parts(), method.clone());

        // Find direct references to this method FQN
        if let Some(refs) = index.references.get(&method_fqn) {
            all_references.extend(refs.clone());
        }

        // Also find references where this method might be called from classes that include the ancestor
        if let Some(refs) =
            find_method_references_in_including_classes(&ancestor_fqn, method, index)
        {
            all_references.extend(refs);
        }
    }

    if all_references.is_empty() {
        None
    } else {
        Some(all_references)
    }
}

/// Find method references in sibling modules (modules included in the same classes)
fn find_method_references_in_sibling_modules(
    module_fqn: &FullyQualifiedName,
    method: &crate::types::ruby_method::RubyMethod,
    index: &crate::indexer::index::RubyIndex,
    kind: MethodKind,
) -> Option<Vec<Location>> {
    let mut all_references = Vec::new();

    // Get all classes/modules that include this module
    let including_classes = index.get_including_classes(module_fqn);

    // For each including class, search in its complete ancestor chain
    for including_class_fqn in including_classes {
        if let Some(refs) =
            find_method_references_in_ancestor_chain(&including_class_fqn, method, index, kind)
        {
            all_references.extend(refs);
        }
    }

    if all_references.is_empty() {
        None
    } else {
        Some(all_references)
    }
}

/// Find method references in classes that include a specific module
fn find_method_references_in_including_classes(
    module_fqn: &FullyQualifiedName,
    method: &crate::types::ruby_method::RubyMethod,
    index: &crate::indexer::index::RubyIndex,
) -> Option<Vec<Location>> {
    let mut all_references = Vec::new();

    // Get all classes that include this module
    let including_classes = index.get_including_classes(module_fqn);

    for including_class_fqn in including_classes {
        // Look for references to the method in the context of the including class
        let method_fqn =
            FullyQualifiedName::method(including_class_fqn.namespace_parts(), method.clone());

        if let Some(refs) = index.references.get(&method_fqn) {
            all_references.extend(refs.clone());
        }
    }

    if all_references.is_empty() {
        None
    } else {
        Some(all_references)
    }
}

/// Find method references by name (fallback for expression receivers)
fn find_method_references_by_name(
    method: &crate::types::ruby_method::RubyMethod,
    index: &crate::indexer::index::RubyIndex,
) -> Option<Vec<Location>> {
    let mut all_references = Vec::new();

    // Search through all method definitions to find references
    if let Some(entries) = index.get_methods_by_name(method) {
        for entry in entries {
            if let EntryKind::Method { .. } = &entry.kind {
                // For each method definition, find its FQN and look for references
                if let Some(refs) = index.references.get(&entry.fqn) {
                    all_references.extend(refs.clone());
                }
            }
        }
    }

    if all_references.is_empty() {
        None
    } else {
        Some(all_references)
    }
}
