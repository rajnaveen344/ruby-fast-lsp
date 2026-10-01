use super::{OracleSupport, UNPROVEN_BLOCK_RECEIVER_GAP};
use crate::simulation::graph::CallShape;

pub(super) fn method_definition_support(shape: &CallShape) -> OracleSupport {
    match shape {
        CallShape::BareInDoBlock
        | CallShape::BareInBraceBlock
        | CallShape::BareInLambda
        | CallShape::BareInProc => OracleSupport::KnownGap(UNPROVEN_BLOCK_RECEIVER_GAP),
        CallShape::Bare
        | CallShape::FrameworkRouteBlock
        | CallShape::Super
        | CallShape::LocalVar { .. }
        | CallShape::Ivar { .. }
        | CallShape::ClassSend
        | CallShape::MethodObject
        | CallShape::InstanceMethodObject
        | CallShape::ClassReceiver { .. }
        | CallShape::ConstructorSend
        | CallShape::StaticSend
        | CallShape::OneHopChain { .. }
        | CallShape::ReceiverLocalVar { .. }
        | CallShape::ArrayBlockParam { .. }
        | CallShape::YieldBlockParam { .. } => OracleSupport::Supported,
    }
}

pub(super) fn method_reference_support(shape: &CallShape) -> OracleSupport {
    match shape {
        CallShape::BareInDoBlock
        | CallShape::BareInBraceBlock
        | CallShape::BareInLambda
        | CallShape::BareInProc => OracleSupport::KnownGap(UNPROVEN_BLOCK_RECEIVER_GAP),
        CallShape::Bare
        | CallShape::FrameworkRouteBlock
        | CallShape::Super
        | CallShape::LocalVar { .. }
        | CallShape::Ivar { .. }
        | CallShape::ClassSend
        | CallShape::MethodObject
        | CallShape::InstanceMethodObject
        | CallShape::ClassReceiver { .. }
        | CallShape::ConstructorSend
        | CallShape::StaticSend
        | CallShape::OneHopChain { .. }
        | CallShape::ReceiverLocalVar { .. }
        | CallShape::ArrayBlockParam { .. }
        | CallShape::YieldBlockParam { .. } => OracleSupport::Supported,
    }
}

pub(super) fn method_hover_support(shape: &CallShape) -> OracleSupport {
    match shape {
        CallShape::BareInDoBlock
        | CallShape::BareInBraceBlock
        | CallShape::BareInLambda
        | CallShape::BareInProc => OracleSupport::KnownGap(UNPROVEN_BLOCK_RECEIVER_GAP),
        CallShape::Bare
        | CallShape::FrameworkRouteBlock
        | CallShape::Super
        | CallShape::LocalVar { .. }
        | CallShape::Ivar { .. }
        | CallShape::ClassSend
        | CallShape::MethodObject
        | CallShape::InstanceMethodObject
        | CallShape::ClassReceiver { .. }
        | CallShape::ConstructorSend
        | CallShape::StaticSend
        | CallShape::OneHopChain { .. }
        | CallShape::ReceiverLocalVar { .. }
        | CallShape::ArrayBlockParam { .. }
        | CallShape::YieldBlockParam { .. } => OracleSupport::Supported,
    }
}
