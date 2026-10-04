//! Enclosing locals a closure body may write.
//!
//! A block or lambda shares its enclosing scope's locals. Whether and how
//! often the body runs belongs to the callee, so after a closure is created
//! every captured local it may assign has no single reaching assignment.

use std::collections::BTreeSet;

use ruby_prism::{
    visit_block_node, visit_lambda_node, BlockNode, ClassNode, DefNode, LambdaNode,
    LocalVariableAndWriteNode, LocalVariableOperatorWriteNode, LocalVariableOrWriteNode,
    LocalVariableTargetNode, LocalVariableWriteNode, ModuleNode, Node, SingletonClassNode, Visit,
};

/// Names of enclosing-scope locals assigned anywhere in a closure `body`,
/// including nested closures. Block parameters, block-locals, and locals first
/// assigned inside the body belong to the closure's own scope and are skipped.
/// `def`, `class`, `module`, and `class << self` open fresh scopes that cannot
/// see the closure's locals.
pub(crate) fn captured_local_writes(body: &Node<'_>) -> BTreeSet<String> {
    let mut finder = CapturedWrites {
        nested_closures: 0,
        names: BTreeSet::new(),
    };
    finder.visit(body);
    finder.names
}

struct CapturedWrites {
    /// Closures entered below the analyzed body. Prism's local `depth` counts
    /// scopes from the write outward, so a write escapes the analyzed closure
    /// only when its depth exceeds this nesting.
    nested_closures: u32,
    names: BTreeSet<String>,
}

impl CapturedWrites {
    fn record(&mut self, name: &[u8], depth: u32) {
        if depth > self.nested_closures {
            self.names
                .insert(String::from_utf8_lossy(name).into_owned());
        }
    }
}

impl<'pr> Visit<'pr> for CapturedWrites {
    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode<'pr>) {
        self.record(node.name().as_slice(), node.depth());
        ruby_prism::visit_local_variable_write_node(self, node);
    }

    fn visit_local_variable_operator_write_node(
        &mut self,
        node: &LocalVariableOperatorWriteNode<'pr>,
    ) {
        self.record(node.name().as_slice(), node.depth());
        ruby_prism::visit_local_variable_operator_write_node(self, node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode<'pr>) {
        self.record(node.name().as_slice(), node.depth());
        ruby_prism::visit_local_variable_or_write_node(self, node);
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode<'pr>) {
        self.record(node.name().as_slice(), node.depth());
        ruby_prism::visit_local_variable_and_write_node(self, node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode<'pr>) {
        self.record(node.name().as_slice(), node.depth());
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

#[cfg(test)]
mod tests {
    use super::captured_local_writes;

    fn writes(source: &str) -> Vec<String> {
        let parsed = ruby_prism::parse(source.as_bytes());
        let program = parsed.node();
        let call = program
            .as_program_node()
            .and_then(|program| program.statements().body().iter().last())
            .and_then(|statement| statement.as_call_node())
            .expect("the last statement is a call");
        let block = call
            .block()
            .and_then(|block| block.as_block_node())
            .expect("the call has a literal block");
        let body = block.body().expect("the block has a body");
        captured_local_writes(&body).into_iter().collect()
    }

    #[test]
    fn keeps_only_enclosing_scope_writes() {
        let source = r#"
total = 0
label = nil
found = nil
items.each do |item; scratch|
  scratch = item
  fresh = item
  total += item
  label ||= item.to_s
  others.each { |other| found = other }
  a, item = 1, 2
  def helper
    total = 1
  end
end
"#;
        assert_eq!(writes(source), vec!["found", "label", "total"]);
    }
}
