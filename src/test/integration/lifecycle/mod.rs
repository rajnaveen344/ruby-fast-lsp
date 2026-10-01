//! Document and indexing lifecycle contracts: deterministic schedules around
//! background collection, commit, dependency refresh, and diagnostic
//! publication, plus exact results across edit recovery.

// Cold coordinator commit and publication paused around editor operations.
mod coordinator_schedules;
// Background collection/commit ordering over the production snapshot guard.
mod commit_interleavings;
// Dependency refresh interleaved with edits, roots, and workspace changes.
mod dependency_refresh;
// Method targets across ancestor edits, partial opens, and closed buffers.
mod hierarchy_edits;
// Complete navigation and rename results that survive edit recovery.
mod exact_results;
// Observation helpers that report, never repair, published and semantic state.
mod observations;
