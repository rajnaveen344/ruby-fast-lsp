use ruby_prism::{ForwardingSuperNode, SuperNode};

use crate::core::{FullyQualifiedName, RubyMethod};
use crate::{Identifier, MethodReceiver};

use super::{IdentifierType, IdentifierVisitor};

impl IdentifierVisitor {
    pub fn process_forwarding_super_node_entry(&mut self, node: &ForwardingSuperNode) {
        self.process_super_keyword(&node.location());
    }

    pub fn process_super_node_entry(&mut self, node: &SuperNode) {
        self.process_super_keyword(&node.keyword_loc());
    }

    fn process_super_keyword(&mut self, location: &ruby_prism::Location) {
        if self.is_result_set() || !self.is_position_in_location(location) {
            return;
        }

        let Some(FullyQualifiedName::Method(_, method)) = self.scope_tracker.current_method_fqn()
        else {
            return;
        };
        let method = RubyMethod::new(method.as_str()).expect(
            "INVARIANT VIOLATED: current method FQN contains invalid Ruby method. \
             This is a bug because RubyMethod validates names at construction. \
             Fix: keep current_method_fqn populated only from RubyMethod values.",
        );

        self.set_result(
            Some(Identifier::RubyMethod {
                namespace: self.scope_tracker.get_ns_stack(),
                receiver: MethodReceiver::Super,
                iden: method,
            }),
            Some(IdentifierType::MethodCall),
            self.scope_tracker.get_ns_stack(),
            Some(0),
        );
    }
}
