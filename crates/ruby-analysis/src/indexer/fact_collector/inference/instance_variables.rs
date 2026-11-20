//! Record every instance-variable assignment the traversal meets.
//!
//! A method that returns an instance variable is typed from all assignments
//! in the object's hierarchy, so each assignment is recorded, proven or not.
//! Only two values are proven here: a scalar literal, and a declared
//! parameter the method never reassigns. Every other assignment is recorded
//! without a type and keeps readers of that variable unknown.

use std::collections::{HashMap, HashSet};

use ustr::Ustr;

use ruby_prism::{
    visit_block_node, visit_lambda_node, BlockNode, ClassNode, DefNode, LambdaNode, ModuleNode,
    Node, ReturnNode, SingletonClassNode, Visit,
};

use crate::core::{FullyQualifiedName, InstanceVariableWrite, NamespaceKind, RubyType};
use crate::indexer::fact_collector::FactCollector;

/// How an assignment combines its value with the variable.
#[derive(Clone, Copy)]
pub(in crate::indexer::fact_collector) enum InstanceVariableWriteKind {
    /// `@name = value`, `@name ||= value`, `@name &&= value`: the variable
    /// afterwards holds the value or what another assignment left there.
    Value,
    /// `@name += value` and multiple-assignment targets: the stored value is
    /// computed elsewhere.
    Computed,
}

#[derive(Default)]
pub(in crate::indexer::fact_collector) struct InstanceVariableEvidence {
    pub(in crate::indexer::fact_collector) writes: Vec<InstanceVariableWrite>,
    /// Enclosing method definitions, innermost last.
    methods: Vec<MethodWrites>,
    /// The number the next `@name ||= value` assignment takes.
    next_memo: u32,
}

/// What one method definition proves about the assignments in its body,
/// keyed by the byte offset where each assignment starts.
struct MethodWrites {
    start_offset: usize,
    end_offset: usize,
    /// The namespace side whose objects run the method.
    owner: FullyQualifiedName,
    /// Whether the method also runs on the namespace's other side, as a
    /// `module_function` copy does.
    runs_on_both_sides: bool,
    parameter_writes: HashMap<usize, RubyType>,
    constructor_writes: HashSet<usize>,
    /// The file-wide number of each `@name ||= value` in the body.
    memos: HashMap<usize, u32>,
}

impl FactCollector {
    /// Analyse a method definition before its body is traversed.
    /// `declared_parameters` are the required parameters whose type the
    /// source declares.
    pub(in crate::indexer::fact_collector) fn begin_instance_variable_method(
        &mut self,
        node: &DefNode,
        owner: FullyQualifiedName,
        runs_on_both_sides: bool,
        declared_parameters: HashMap<Vec<u8>, RubyType>,
        is_constructor: bool,
    ) {
        let start_offset = node.location().start_offset();
        self.instance_variables
            .methods
            .retain(|method| start_offset < method.end_offset);

        let mut finder = ParameterWrites::default();
        if let Some(parameters) = node.parameters() {
            finder.visit_parameters_node(&parameters);
        }
        if let Some(body) = node.body() {
            finder.visit(&body);
        }
        let mut memos = HashMap::new();
        for offset in std::mem::take(&mut finder.memos) {
            memos.insert(offset, self.instance_variables.next_memo);
            self.instance_variables.next_memo += 1;
        }
        let parameter_writes = finder
            .candidates
            .into_iter()
            .filter(|(_, name)| !finder.reassigned.contains(name))
            .filter_map(|(offset, name)| Some((offset, declared_parameters.get(&name)?.clone())))
            .filter(|(_, ruby_type)| !RubyType::contains_unknown(ruby_type))
            .collect();

        self.instance_variables.methods.push(MethodWrites {
            start_offset,
            end_offset: node.location().end_offset(),
            owner,
            runs_on_both_sides,
            parameter_writes,
            memos,
            constructor_writes: if is_constructor {
                constructor_writes(node)
            } else {
                HashSet::new()
            },
        });
    }

    /// Record one assignment starting at `offset`.
    pub(in crate::indexer::fact_collector) fn record_instance_variable_write(
        &mut self,
        name: &str,
        offset: usize,
        value: Option<&Node>,
        kind: InstanceVariableWriteKind,
    ) {
        let method = self
            .scope_tracker
            .current_method()
            .is_some()
            .then(|| {
                self.instance_variables
                    .methods
                    .iter()
                    .rev()
                    .find(|method| offset < method.end_offset)
            })
            .flatten();
        let (owner, exact_owner) = match method {
            Some(method) => (method.owner.clone(), !method.runs_on_both_sides),
            // Outside a method the assignment runs on the implicit receiver,
            // but a block may be handed to a method that later runs it on an
            // instance, so it may reach either side.
            None => {
                let (namespace, kind) = self.scope_tracker.implicit_receiver_context();
                (
                    FullyQualifiedName::namespace_with_kind(namespace, kind),
                    false,
                )
            }
        };
        let proven = match kind {
            InstanceVariableWriteKind::Computed => None,
            InstanceVariableWriteKind::Value => value.and_then(scalar_literal_type).or_else(|| {
                method.and_then(|method| method.parameter_writes.get(&offset).cloned())
            }),
        };
        let in_constructor =
            method.is_some_and(|method| method.constructor_writes.contains(&offset));
        let memo = method.and_then(|method| method.memos.get(&offset).copied());
        if !exact_owner {
            let other_kind = match owner.namespace_kind() {
                Some(NamespaceKind::Singleton) => NamespaceKind::Instance,
                Some(NamespaceKind::Instance) | None => NamespaceKind::Singleton,
            };
            self.instance_variables.writes.push(InstanceVariableWrite {
                owner: FullyQualifiedName::namespace_with_kind(owner.namespace_parts(), other_kind),
                name: name.into(),
                proven: None,
                in_constructor: false,
                memo: None,
            });
        }
        self.instance_variables.writes.push(InstanceVariableWrite {
            owner,
            name: name.into(),
            proven,
            in_constructor,
            memo,
        });
    }

    /// The owner of instance variables read by the method defined at `node`,
    /// when the method runs on one side of its namespace only, and the type
    /// this file's assignments so far prove for each of that owner's
    /// variables. The engine confirms every use against the whole project.
    pub(in crate::indexer::fact_collector) fn instance_variables_of_method(
        &self,
        node: &DefNode,
    ) -> (Option<FullyQualifiedName>, HashMap<Ustr, RubyType>) {
        let start_offset = node.location().start_offset();
        let Some(owner) = self
            .instance_variables
            .methods
            .iter()
            .rev()
            .find(|method| method.start_offset == start_offset)
            .filter(|method| !method.runs_on_both_sides)
            .map(|method| method.owner.clone())
        else {
            return (None, HashMap::new());
        };
        let mut members: HashMap<Ustr, Option<Vec<RubyType>>> = HashMap::new();
        let mut in_constructor = HashSet::new();
        for write in &self.instance_variables.writes {
            if write.owner != owner {
                continue;
            }
            if write.name.as_str() == InstanceVariableWrite::ANY_NAME {
                return (Some(owner), HashMap::new());
            }
            if write.in_constructor {
                in_constructor.insert(write.name);
            }
            let entry = members
                .entry(write.name)
                .or_insert_with(|| Some(Vec::new()));
            match (&write.proven, entry.as_mut()) {
                (Some(proven), Some(types)) => types.push(proven.clone()),
                (None, _) => *entry = None,
                (Some(_), None) => {}
            }
        }
        let types = members
            .into_iter()
            .filter_map(|(name, types)| {
                let mut types = types?;
                if !in_constructor.contains(&name) {
                    types.push(RubyType::nil_class());
                }
                Some((name, RubyType::union(types)))
            })
            .collect();
        (Some(owner), types)
    }

    /// For each `@name ||= value` in the method defined at `node`, keyed by
    /// where it starts: its number, and the type every other assignment of
    /// the variable in this file so far proves. A variable nothing else
    /// assigns holds nil until the method runs. A variable some other
    /// assignment leaves unproven is left out.
    pub(in crate::indexer::fact_collector) fn memoised_instance_variables_of_method(
        &self,
        node: &DefNode,
        owner: &FullyQualifiedName,
    ) -> HashMap<usize, (u32, RubyType)> {
        let start_offset = node.location().start_offset();
        let Some(method) = self
            .instance_variables
            .methods
            .iter()
            .rev()
            .find(|method| method.start_offset == start_offset)
        else {
            return HashMap::new();
        };
        let mut finder = MemoNames::default();
        if let Some(body) = node.body() {
            finder.visit(&body);
        }
        finder
            .names
            .into_iter()
            .filter_map(|(offset, name)| {
                let memo = *method.memos.get(&offset)?;
                let others = self.other_assignments_type(owner, &name, memo)?;
                Some((offset, (memo, others)))
            })
            .collect()
    }

    fn other_assignments_type(
        &self,
        owner: &FullyQualifiedName,
        name: &str,
        memo: u32,
    ) -> Option<RubyType> {
        let mut types = Vec::new();
        let mut in_constructor = false;
        for write in &self.instance_variables.writes {
            if write.owner != *owner || write.memo == Some(memo) {
                continue;
            }
            if write.name.as_str() == InstanceVariableWrite::ANY_NAME {
                return None;
            }
            if write.name.as_str() != name {
                continue;
            }
            types.push(write.proven.clone()?);
            in_constructor |= write.in_constructor;
        }
        if !in_constructor {
            types.push(RubyType::nil_class());
        }
        Some(RubyType::union(types))
    }

    /// Record a generated writer method, which stores whatever its caller
    /// passes.
    pub(in crate::indexer::fact_collector) fn record_generated_instance_variable_writer(
        &mut self,
        owner: FullyQualifiedName,
        name: &str,
    ) {
        self.instance_variables.writes.push(InstanceVariableWrite {
            owner,
            name: name.into(),
            proven: None,
            in_constructor: false,
            memo: None,
        });
    }
}

/// The class of a literal whose value has one class whatever runs.
fn scalar_literal_type(node: &Node) -> Option<RubyType> {
    match node {
        Node::StringNode { .. } => Some(RubyType::string()),
        Node::SymbolNode { .. } => Some(RubyType::symbol()),
        Node::IntegerNode { .. } => Some(RubyType::integer()),
        Node::FloatNode { .. } => Some(RubyType::float()),
        Node::TrueNode { .. } => Some(RubyType::true_class()),
        Node::FalseNode { .. } => Some(RubyType::false_class()),
        Node::NilNode { .. } => Some(RubyType::nil_class()),
        _ => None,
    }
}

/// Plain assignments every completed run of the constructor reaches: direct
/// statements of its body before the first statement that may return.
fn constructor_writes(node: &DefNode) -> HashSet<usize> {
    let mut writes = HashSet::new();
    let Some(statements) = node.body().and_then(|body| body.as_statements_node()) else {
        return writes;
    };
    for statement in statements.body().iter() {
        let mut finder = ReturnFinder { found: false };
        finder.visit(&statement);
        if finder.found {
            break;
        }
        if statement.as_instance_variable_write_node().is_some() {
            writes.insert(statement.location().start_offset());
        }
    }
    writes
}

struct ReturnFinder {
    found: bool,
}

impl<'pr> Visit<'pr> for ReturnFinder {
    fn visit_return_node(&mut self, _node: &ReturnNode<'pr>) {
        self.found = true;
    }

    fn visit_def_node(&mut self, _node: &DefNode<'pr>) {}

    fn visit_lambda_node(&mut self, _node: &LambdaNode<'pr>) {}
}

/// Locals a method body assigns, and the instance-variable assignments whose
/// value reads a local of the method's own scope.
#[derive(Default)]
struct ParameterWrites {
    /// Closures entered below the method body. A read reaches the method's
    /// own scope only when its depth equals this nesting.
    nested_closures: u32,
    reassigned: HashSet<Vec<u8>>,
    candidates: Vec<(usize, Vec<u8>)>,
    /// Where each `@name ||= value` starts, in source order.
    memos: Vec<usize>,
}

impl ParameterWrites {
    fn candidate(&mut self, offset: usize, value: &Node) {
        if let Some(read) = value.as_local_variable_read_node() {
            if read.depth() == self.nested_closures {
                self.candidates
                    .push((offset, read.name().as_slice().to_vec()));
            }
        }
    }
}

impl<'pr> Visit<'pr> for ParameterWrites {
    fn visit_instance_variable_write_node(
        &mut self,
        node: &ruby_prism::InstanceVariableWriteNode<'pr>,
    ) {
        self.candidate(node.location().start_offset(), &node.value());
        ruby_prism::visit_instance_variable_write_node(self, node);
    }

    fn visit_instance_variable_or_write_node(
        &mut self,
        node: &ruby_prism::InstanceVariableOrWriteNode<'pr>,
    ) {
        self.memos.push(node.location().start_offset());
        self.candidate(node.location().start_offset(), &node.value());
        ruby_prism::visit_instance_variable_or_write_node(self, node);
    }

    fn visit_instance_variable_and_write_node(
        &mut self,
        node: &ruby_prism::InstanceVariableAndWriteNode<'pr>,
    ) {
        self.candidate(node.location().start_offset(), &node.value());
        ruby_prism::visit_instance_variable_and_write_node(self, node);
    }

    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
        self.reassigned.insert(node.name().as_slice().to_vec());
        ruby_prism::visit_local_variable_write_node(self, node);
    }

    fn visit_local_variable_operator_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableOperatorWriteNode<'pr>,
    ) {
        self.reassigned.insert(node.name().as_slice().to_vec());
        ruby_prism::visit_local_variable_operator_write_node(self, node);
    }

    fn visit_local_variable_or_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableOrWriteNode<'pr>,
    ) {
        self.reassigned.insert(node.name().as_slice().to_vec());
        ruby_prism::visit_local_variable_or_write_node(self, node);
    }

    fn visit_local_variable_and_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableAndWriteNode<'pr>,
    ) {
        self.reassigned.insert(node.name().as_slice().to_vec());
        ruby_prism::visit_local_variable_and_write_node(self, node);
    }

    fn visit_local_variable_target_node(
        &mut self,
        node: &ruby_prism::LocalVariableTargetNode<'pr>,
    ) {
        self.reassigned.insert(node.name().as_slice().to_vec());
    }

    fn visit_block_node(&mut self, node: &BlockNode<'pr>) {
        self.nested_closures += 1;
        visit_block_node(self, node);
        self.nested_closures -= 1;
    }

    fn visit_lambda_node(&mut self, node: &LambdaNode<'pr>) {
        self.nested_closures += 1;
        visit_lambda_node(self, node);
        self.nested_closures -= 1;
    }

    fn visit_def_node(&mut self, _node: &DefNode<'pr>) {}

    fn visit_class_node(&mut self, _node: &ClassNode<'pr>) {}

    fn visit_module_node(&mut self, _node: &ModuleNode<'pr>) {}

    fn visit_singleton_class_node(&mut self, _node: &SingletonClassNode<'pr>) {}
}

/// The variable each `@name ||= value` of a method body assigns, by where
/// the assignment starts. Nested definitions run on their own.
#[derive(Default)]
struct MemoNames {
    names: Vec<(usize, String)>,
}

impl<'pr> Visit<'pr> for MemoNames {
    fn visit_instance_variable_or_write_node(
        &mut self,
        node: &ruby_prism::InstanceVariableOrWriteNode<'pr>,
    ) {
        self.names.push((
            node.location().start_offset(),
            String::from_utf8_lossy(node.name().as_slice()).to_string(),
        ));
        ruby_prism::visit_instance_variable_or_write_node(self, node);
    }

    fn visit_def_node(&mut self, _node: &DefNode<'pr>) {}

    fn visit_class_node(&mut self, _node: &ClassNode<'pr>) {}

    fn visit_module_node(&mut self, _node: &ModuleNode<'pr>) {}

    fn visit_singleton_class_node(&mut self, _node: &SingletonClassNode<'pr>) {}
}
