//! Flow diagnostics: unreachable code, inconsistent returns, and calls on
//! definitely nil receivers.

mod inconsistent_return;
mod nil_call;
mod unreachable_code;
