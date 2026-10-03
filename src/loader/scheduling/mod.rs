//! Indexing progress: the indexing queue, status reporting, and demand-driven
//! navigation indexing. The CPU/task/memory/I/O governor lives in
//! `crate::utils::admission`.

pub(crate) mod navigation_demand;
pub mod scheduler;
pub mod status;

#[cfg(test)]
pub(crate) mod test_schedule;
