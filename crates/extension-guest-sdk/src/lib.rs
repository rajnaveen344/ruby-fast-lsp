//! Typed Rust guest surface for Ruby Fast LSP Wasm extensions.
//!
//! Guests implement [`GuestExtension`] and invoke [`export_extension!`]. The
//! generated exports are the same bounded JSON-over-linear-memory ABI consumed
//! by `ruby-fast-lsp-extension-wasm-host`; no engine, server, or LSP protocol
//! handle crosses the guest boundary.

use ruby_fast_lsp_extension_api::{
    CallContext, Extension, ExtensionEvent, ExtensionOutput, ABI_VERSION,
};
use serde::Serialize;

/// Stateful typed contract implemented by Rust-authored Wasm guests.
pub trait GuestExtension: Send + 'static {
    fn indexed_call_names(&self) -> &'static [&'static str];

    fn index_call(&mut self, context: &CallContext) -> ExtensionOutput;

    fn handle_event(&mut self, event: &ExtensionEvent) -> ExtensionOutput {
        if event.event == "index.call.enter" {
            let context = event.call.as_ref().expect(
                "INVARIANT VIOLATED: index.call.enter omitted CallContext. This is a host/guest ABI bug because call events require their typed call payload. Fix: encode CallContext on every index.call.enter event.",
            );
            return self.index_call(context);
        }
        ExtensionOutput::index_patches(Vec::new())
    }
}

/// Existing stateless/native extensions can be compiled as typed Wasm guests
/// without rewriting their semantic implementation.
impl<T> GuestExtension for T
where
    T: Extension + Send + 'static,
{
    fn indexed_call_names(&self) -> &'static [&'static str] {
        Extension::indexed_call_names(self)
    }

    fn index_call(&mut self, context: &CallContext) -> ExtensionOutput {
        Extension::index_call_output(self, context)
    }
}

#[doc(hidden)]
pub fn decode_call(input: &[u8]) -> CallContext {
    serde_json::from_slice(input).expect(
        "INVARIANT VIOLATED: Rust guest received invalid CallContext JSON. This is a host/guest ABI bug because the host must serialize the versioned extension-api type. Fix: keep host and guest ABI versions synchronized.",
    )
}

#[doc(hidden)]
pub fn decode_event(input: &[u8]) -> ExtensionEvent {
    serde_json::from_slice(input).expect(
        "INVARIANT VIOLATED: Rust guest received invalid ExtensionEvent JSON. This is a host/guest ABI bug because the host must serialize the versioned extension-api type. Fix: keep host and guest ABI versions synchronized.",
    )
}

#[doc(hidden)]
pub fn encode_json<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).expect(
        "INVARIANT VIOLATED: typed Rust guest output failed JSON serialization. This is a guest SDK bug because extension-api domain values must be serializable. Fix: keep every public ABI value serde-compatible.",
    )
}

#[doc(hidden)]
pub fn empty_output() -> ExtensionOutput {
    ExtensionOutput::index_patches(Vec::new())
}

#[doc(hidden)]
pub const fn abi_version() -> i32 {
    ABI_VERSION as i32
}

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub fn allocate(len: i32) -> *mut u8 {
    assert!(
        len >= 0,
        "INVARIANT VIOLATED: Rust guest alloc received a negative length. This is a host ABI bug because payload lengths are non-negative i32 values. Fix: validate the host payload length before allocation."
    );
    let bytes = vec![0_u8; len as usize].into_boxed_slice();
    Box::into_raw(bytes) as *mut u8
}

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub unsafe fn deallocate(ptr: *mut u8, len: i32) {
    assert!(
        len >= 0,
        "INVARIANT VIOLATED: Rust guest dealloc received a negative length. This is a host ABI bug because payload lengths are non-negative i32 values. Fix: preserve the allocation length across the call boundary."
    );
    let slice = std::ptr::slice_from_raw_parts_mut(ptr, len as usize);
    drop(Box::from_raw(slice));
}

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub fn input_bytes<'a>(ptr: *const u8, len: i32) -> &'a [u8] {
    assert!(
        len >= 0,
        "INVARIANT VIOLATED: Rust guest input received a negative length. This is a host ABI bug because payload lengths are non-negative i32 values. Fix: validate the host payload length before invoking the guest."
    );
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub fn return_bytes(bytes: Vec<u8>) -> i64 {
    assert!(
        bytes.len() <= i32::MAX as usize,
        "INVARIANT VIOLATED: Rust guest output exceeds the i32 ABI length. This is a guest bug because the host enforces a much smaller bounded output. Fix: emit bounded semantic patches."
    );
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len() as u32;
    let ptr = Box::into_raw(boxed) as *mut u8 as u32;
    ((ptr as u64) << 32 | len as u64) as i64
}

/// Export a stateful Rust extension through the Ruby Fast LSP Wasm ABI.
///
/// The factory is evaluated once per Wasm instance. All calls are serialized by
/// the host, while the mutex also makes the generated static valid Rust state.
#[macro_export]
macro_rules! export_extension {
    ($factory:path) => {
        fn ruby_fast_lsp_guest() -> &'static std::sync::Mutex<Box<dyn $crate::GuestExtension>> {
            static GUEST: std::sync::OnceLock<
                std::sync::Mutex<Box<dyn $crate::GuestExtension>>,
            > = std::sync::OnceLock::new();
            GUEST.get_or_init(|| std::sync::Mutex::new(Box::new($factory())))
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn abi_version() -> i32 {
            $crate::abi_version()
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn alloc(len: i32) -> *mut u8 {
            $crate::allocate(len)
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: i32) {
            $crate::deallocate(ptr, len)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn indexed_call_names() -> i64 {
            let guest = ruby_fast_lsp_guest().lock().expect(
                "INVARIANT VIOLATED: Rust guest mutex was poisoned. This is a guest bug because a prior callback panicked. Fix: remove the panic and allow the host to isolate failures as Wasm traps.",
            );
            $crate::return_bytes($crate::encode_json(&guest.indexed_call_names()))
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn index_call(ptr: *const u8, len: i32) -> i64 {
            let context = $crate::decode_call($crate::input_bytes(ptr, len));
            let output = ruby_fast_lsp_guest().lock().expect(
                "INVARIANT VIOLATED: Rust guest mutex was poisoned. This is a guest bug because a prior callback panicked. Fix: remove the panic and allow the host to isolate failures as Wasm traps.",
            ).index_call(&context);
            $crate::return_bytes($crate::encode_json(&output.index_patches))
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn handle_event(ptr: *const u8, len: i32) -> i64 {
            let event = $crate::decode_event($crate::input_bytes(ptr, len));
            let output = ruby_fast_lsp_guest().lock().expect(
                "INVARIANT VIOLATED: Rust guest mutex was poisoned. This is a guest bug because a prior callback panicked. Fix: remove the panic and allow the host to isolate failures as Wasm traps.",
            ).handle_event(&event);
            $crate::return_bytes($crate::encode_json(&output))
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_fast_lsp_extension_api::{NamespaceKind, Receiver, SourcePosition, SourceRange};

    struct Fixture;

    impl Extension for Fixture {
        fn id(&self) -> &'static str {
            "fixture"
        }

        fn indexed_call_names(&self) -> &'static [&'static str] {
            &["fixture"]
        }

        fn index_call(
            &self,
            _context: &CallContext,
        ) -> Vec<ruby_fast_lsp_extension_api::IndexPatch> {
            Vec::new()
        }
    }

    fn context() -> CallContext {
        let range = SourceRange {
            start: SourcePosition {
                line: 0,
                character: 0,
            },
            end: SourcePosition {
                line: 0,
                character: 7,
            },
        };
        CallContext {
            project: None,
            method_name: "fixture".to_string(),
            receiver: Receiver::None,
            arguments: Vec::new(),
            current_namespace: Vec::new(),
            namespace_kind: NamespaceKind::Instance,
            call_range: range,
            block_range: None,
            message_range: range,
            resolved_callees: Vec::new(),
            enclosing_calls: Vec::new(),
        }
    }

    #[test]
    fn stateless_extension_adapts_to_typed_guest_contract() {
        let mut fixture = Fixture;
        assert_eq!(GuestExtension::indexed_call_names(&fixture), &["fixture"]);
        assert_eq!(
            GuestExtension::index_call(&mut fixture, &context()),
            empty_output()
        );
    }

    #[test]
    fn typed_json_round_trip_uses_public_abi_types() {
        let expected = context();
        assert_eq!(decode_call(&encode_json(&expected)), expected);
        let event = ExtensionEvent {
            event: "index.call.enter".to_string(),
            call: Some(expected),
            document: None,
            project: None,
            settings: None,
            files: None,
            process_results: None,
        };
        assert_eq!(decode_event(&encode_json(&event)), event);
    }
}
