use super::oracle_support::{
    method_definition_support, method_hover_support, method_reference_support,
};
use super::source_text::{
    constant_ref_cursor_offset, constant_ref_text, indent_len, yield_helper_name,
};
use super::{CallSite, ConstantRefSite, FileRenderer, SourcePos};
use crate::simulation::graph::{CallShape, MethodKind, MethodTarget, NamespaceSpec};

impl FileRenderer {
    pub(super) fn render_constant_ref(
        &mut self,
        lexical_scope: &str,
        caller: &MethodTarget,
        constant: &super::graph::ConstantRefSpec,
        depth: usize,
    ) {
        let text = constant_ref_text(lexical_scope, constant);
        self.map.constant_refs.push(ConstantRefSite {
            caller: caller.clone(),
            lexical_scope: lexical_scope.to_string(),
            target: constant.fqn.clone(),
            text: text.clone(),
            shape: constant.shape.clone(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + constant_ref_cursor_offset(&constant.shape, &text))
                    as u32,
            },
        });
        self.push_line(depth, &text);
    }

    pub(super) fn render_call(
        &mut self,
        caller: &MethodTarget,
        target: &MethodTarget,
        shape: &CallShape,
        depth: usize,
    ) {
        match shape {
            CallShape::Bare => {
                self.record_call(caller, target, shape, depth, "");
                self.push_line(depth, &target.name);
            }
            CallShape::BareInDoBlock => {
                self.push_line(depth, "[1].each do |_v|");
                self.record_call(caller, target, shape, depth + 1, "");
                self.push_line(depth + 1, &target.name);
                self.push_line(depth, "end");
            }
            CallShape::BareInBraceBlock => {
                let receiver = "[1].map { |_v| ";
                self.record_call(caller, target, shape, depth, receiver);
                self.push_line(depth, &format!("{}{} }}", receiver, target.name));
            }
            CallShape::BareInLambda => {
                let receiver = "lambda_probe = -> { ";
                self.record_call(caller, target, shape, depth, receiver);
                self.push_line(depth, &format!("{}{} }}", receiver, target.name));
            }
            CallShape::BareInProc => {
                let receiver = "Proc.new { ";
                self.record_call(caller, target, shape, depth, receiver);
                self.push_line(depth, &format!("{}{} }}", receiver, target.name));
            }
            CallShape::FrameworkRouteBlock => {
                self.push_line(depth, "get \"/synthetic\" do");
                self.record_call(caller, target, shape, depth + 1, "");
                self.push_line(depth + 1, &target.name);
                self.push_line(depth, "end");
            }
            CallShape::Super => {
                self.record_call(caller, target, shape, depth, "");
                self.push_line(depth, "super");
            }
            CallShape::LocalVar { name } => {
                self.push_line(depth, &format!("{} = {}.new", name, target.owner));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::Ivar { name } => {
                let ivar = format!("@{}", name);
                self.push_line(depth, &format!("{} = {}.new", ivar, target.owner));
                let receiver = format!("{}.", ivar);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ClassSend => {
                let receiver = format!("{}.", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::MethodObject => {
                let receiver = match target.kind {
                    MethodKind::Class => format!("{}.method(:", target.owner),
                    MethodKind::Instance => "method(:".to_string(),
                };
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{})", receiver, target.name));
            }
            CallShape::InstanceMethodObject => {
                let receiver = format!("{}.instance_method(:", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{})", receiver, target.name));
            }
            CallShape::ClassReceiver { receiver_owner } => {
                let receiver = format!("{}.", receiver_owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ConstructorSend => {
                let receiver = format!("{}.new.", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::StaticSend => {
                let receiver = format!("{}.new.send(:", target.owner);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{})", receiver, target.name));
            }
            CallShape::OneHopChain {
                name,
                receiver_owner,
                hop_method,
            } => {
                self.push_line(depth, &format!("{} = {}.new", name, receiver_owner));
                let receiver = format!("{}.{}.", name, hop_method);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ReceiverLocalVar {
                name,
                receiver_owner,
            } => {
                self.push_line(depth, &format!("{} = {}.new", name, receiver_owner));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth, &receiver);
                self.push_line(depth, &format!("{}{}", receiver, target.name));
            }
            CallShape::ArrayBlockParam { name } => {
                self.push_line(depth, &format!("[{}.new].each do |{}|", target.owner, name));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth + 1, &receiver);
                self.push_line(depth + 1, &format!("{}{}", receiver, target.name));
                self.push_line(depth, "end");
            }
            CallShape::YieldBlockParam { name } => {
                let helper = yield_helper_name(target);
                self.push_line(depth, &format!("def {}", helper));
                self.push_line(depth + 1, &format!("yield {}.new", target.owner));
                self.push_line(depth, "end");
                self.push_line(depth, &format!("{} do |{}|", helper, name));
                let receiver = format!("{}.", name);
                self.record_call(caller, target, shape, depth + 1, &receiver);
                self.push_line(depth + 1, &format!("{}{}", receiver, target.name));
                self.push_line(depth, "end");
            }
        }
    }

    pub(super) fn render_route_call(
        &mut self,
        namespace: &NamespaceSpec,
        call: &super::graph::CallSpec,
        depth: usize,
    ) {
        let caller = MethodTarget {
            owner: namespace.fqn.clone(),
            name: "__sim_route__".to_string(),
            kind: MethodKind::Instance,
        };
        self.render_call(&caller, &call.target, &call.shape, depth);
    }

    fn record_call(
        &mut self,
        caller: &MethodTarget,
        target: &MethodTarget,
        shape: &CallShape,
        depth: usize,
        receiver: &str,
    ) {
        self.map.calls.push(CallSite {
            caller: caller.clone(),
            target: target.clone(),
            shape: shape.clone(),
            pos: SourcePos {
                file: self.file.clone(),
                line: self.line,
                character: (indent_len(depth) + receiver.len()) as u32,
            },
            shape_name: shape.label(),
            definition_support: method_definition_support(shape),
            reference_support: method_reference_support(shape),
            hover_support: method_hover_support(shape),
        });

        assert!(
            !target.name.is_empty(),
            "INVARIANT VIOLATED: generated call has empty method name. This is a bug because call positions must point at a Ruby identifier. Fix: validate MethodTarget parsing."
        );
    }
}
