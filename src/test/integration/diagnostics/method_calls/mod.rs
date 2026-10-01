//! Method-call diagnostics: arity, keyword arguments, misspelled methods,
//! `super` calls, and unresolved methods on expression receivers.

mod clean_receivers;
mod expr_receiver_unresolved;
mod missing_kwarg;
mod misspelled_method;
mod super_calls;
mod unknown_kwarg;
mod wrong_arity;
