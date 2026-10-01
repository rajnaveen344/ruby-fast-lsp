//! Open-document constant demand used to prioritize project files and gems.

use crate::server::RubyLanguageServer;
use ruby_prism::{ConstantPathNode, ConstantReadNode, Visit};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::Path;

const MAX_ACTIVE_PROJECT_FILE_PRIORITY_KEYS: usize = 8;

#[derive(Default)]
struct ActiveDocumentConstantVisitor {
    pub(super) dependency_roots: HashSet<String>,
    pub(super) project_terminals: Vec<String>,
    seen_project_terminals: HashSet<String>,
    constant_path_depth: usize,
}

impl ActiveDocumentConstantVisitor {
    fn push_project_terminal(&mut self, key: String) {
        if self.seen_project_terminals.insert(key.clone()) {
            self.project_terminals.push(key);
        }
    }
}

impl<'pr> Visit<'pr> for ActiveDocumentConstantVisitor {
    fn visit_constant_read_node(&mut self, node: &ConstantReadNode<'pr>) {
        let key = dependency_priority_key(&String::from_utf8_lossy(node.name().as_slice()));
        self.dependency_roots.insert(key.clone());
        if self.constant_path_depth == 0 {
            self.push_project_terminal(key);
        }
        ruby_prism::visit_constant_read_node(self, node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode<'pr>) {
        if self.constant_path_depth == 0 {
            if let Some(name) = node.name() {
                self.push_project_terminal(dependency_priority_key(&String::from_utf8_lossy(
                    name.as_slice(),
                )));
            }
        }
        self.constant_path_depth += 1;
        ruby_prism::visit_constant_path_node(self, node);
        self.constant_path_depth = self.constant_path_depth.checked_sub(1).expect(
            "INVARIANT VIOLATED: active-document constant-path traversal depth underflowed. \
                 This is a bug because every constant-path visit increments exactly once before \
                 recursive traversal. Fix: keep traversal depth updates paired around the default \
                 Prism visitor.",
        );
    }
}

#[derive(Clone, Default)]
pub(super) struct ActiveDocumentPriorityKeys {
    pub(super) dependency_roots: HashSet<String>,
    pub(super) project_terminals: Vec<String>,
}

impl ActiveDocumentPriorityKeys {
    fn extend(&mut self, other: Self) {
        self.dependency_roots.extend(other.dependency_roots);
        for terminal in other.project_terminals {
            if self.project_terminals.len() == MAX_ACTIVE_PROJECT_FILE_PRIORITY_KEYS {
                break;
            }
            if !self.project_terminals.contains(&terminal) {
                self.project_terminals.push(terminal);
            }
        }
    }
}

pub(crate) fn dependency_priority_key(name: &str) -> String {
    crate::navigation_demand::normalize_navigation_key(name)
}

pub(super) fn active_document_constant_priority_keys(source: &str) -> ActiveDocumentPriorityKeys {
    let source = ruby_analysis::indexer::mask_shebang(source);
    let parse = ruby_prism::parse(source.as_bytes());
    let mut visitor = ActiveDocumentConstantVisitor::default();
    visitor.visit(&parse.node());
    visitor
        .project_terminals
        .truncate(MAX_ACTIVE_PROJECT_FILE_PRIORITY_KEYS);
    ActiveDocumentPriorityKeys {
        dependency_roots: visitor.dependency_roots,
        project_terminals: visitor.project_terminals,
    }
}

pub(super) fn open_project_constant_priority_keys(
    server: &RubyLanguageServer,
    workspace_root: &Path,
) -> ActiveDocumentPriorityKeys {
    let mut documents = server
        .documents
        .read()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    documents.sort_by(|left, right| left.read().uri.cmp(&right.read().uri));
    let mut priority_keys = ActiveDocumentPriorityKeys::default();
    for document in documents {
        let document = document.read();
        let Some(workspace) = server.workspace_for_uri(&document.uri) else {
            continue;
        };
        if workspace.root_path != workspace_root {
            continue;
        }
        priority_keys.extend(active_document_constant_priority_keys(
            document.analysis_content(),
        ));
    }
    priority_keys
}

pub(super) fn prioritize_locked_gem_names(
    names: Vec<String>,
    priority_keys: &HashSet<String>,
) -> Vec<String> {
    let (prioritized, exhaustive): (Vec<_>, Vec<_>) = names
        .into_iter()
        .partition(|name| priority_keys.contains(&dependency_priority_key(name)));
    prioritized.into_iter().chain(exhaustive).collect()
}

pub(super) fn prioritize_demanded_gem_names(
    remaining: &mut VecDeque<String>,
    demand_keys: &[String],
) -> BTreeMap<String, Vec<String>> {
    let mut unmatched_names = std::mem::take(remaining).into_iter().collect::<Vec<_>>();
    let mut prioritized_names = Vec::new();
    let mut matched = BTreeMap::<String, Vec<String>>::new();
    for key in demand_keys {
        let Some(index) = unmatched_names
            .iter()
            .position(|name| dependency_priority_key(name) == *key)
        else {
            continue;
        };
        let name = unmatched_names.remove(index);
        matched.entry(name.clone()).or_default().push(key.clone());
        prioritized_names.push(name);
    }
    remaining.extend(prioritized_names.into_iter().chain(unmatched_names));
    matched
}
