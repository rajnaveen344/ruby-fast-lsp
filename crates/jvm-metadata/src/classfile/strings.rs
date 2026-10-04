//! Shared string storage for parsed classfile metadata.
//!
//! Member descriptors, owner names, exception names, and parameter names
//! repeat heavily across the classes of one archive. Parsing and decoding
//! route every retained string through one interner per archive so equal
//! strings share a single allocation.

use super::model::{ClassFile, MemberInfo, MethodParameter};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct JvmStringInterner {
    strings: HashSet<Arc<str>>,
}

impl JvmStringInterner {
    pub fn intern(&mut self, value: &str) -> Arc<str> {
        if let Some(existing) = self.strings.get(value) {
            return Arc::clone(existing);
        }
        let shared: Arc<str> = Arc::from(value);
        self.strings.insert(Arc::clone(&shared));
        shared
    }

    fn share(&mut self, value: &mut Arc<str>) {
        let shared = self.intern(value);
        *value = shared;
    }
}

impl ClassFile {
    /// Replace every retained string with the interner's shared copy. Decoded
    /// metadata arrives with one allocation per string occurrence.
    pub fn share_strings(&mut self, interner: &mut JvmStringInterner) {
        interner.share(&mut self.name);
        if let Some(super_name) = self.super_name.as_mut() {
            interner.share(super_name);
        }
        for interface in self.interfaces.iter_mut() {
            interner.share(interface);
        }
        for member in self.fields.iter_mut().chain(self.methods.iter_mut()) {
            share_member_strings(member, interner);
        }
        if let Some(source_file) = self.source_file.as_mut() {
            interner.share(source_file);
        }
    }
}

fn share_member_strings(member: &mut MemberInfo, interner: &mut JvmStringInterner) {
    interner.share(&mut member.name);
    interner.share(&mut member.descriptor);
    for exception in member.exceptions.iter_mut() {
        interner.share(exception);
    }
    for MethodParameter { name } in member.parameters.iter_mut() {
        interner.share(name);
    }
}
