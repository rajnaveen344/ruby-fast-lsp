//! Analyzer modules — each diagnostic is its own module consuming
//! `&dyn SymbolTable` plus explicit context (document, namespace, …)
//! instead of being fused into a god-class visitor.

pub mod raise_non_exception;
