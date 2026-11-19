//! Method returns a file-owned equation relied on while it was collected.
//!
//! A method body is evaluated once against the returns other methods had at
//! that moment. The proven base keeps those answers; an observation records
//! the lookup and its answer so the solver can withhold the base when the
//! same lookup no longer gives that answer.

use std::mem::size_of;

use crate::core::storage::memory_estimate::{fqn_heap_bytes, ruby_type_heap_bytes};
use crate::core::{FullyQualifiedName, RubyMethod, RubyType};

/// Which receiver methods the recorded lookup could see.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservedReceiverAccess {
    Any,
    Protected(FullyQualifiedName),
    Public,
}

/// One dispatched return lookup and the type it answered with.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MethodReturnObservation {
    pub receiver: FullyQualifiedName,
    pub method: RubyMethod,
    pub access: ObservedReceiverAccess,
    pub observed: RubyType,
}

impl MethodReturnObservation {
    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        size_of::<Self>()
            + fqn_heap_bytes(&self.receiver)
            + ruby_type_heap_bytes(&self.observed)
            + match &self.access {
                ObservedReceiverAccess::Protected(caller) => fqn_heap_bytes(caller),
                ObservedReceiverAccess::Any | ObservedReceiverAccess::Public => 0,
            }
    }
}
