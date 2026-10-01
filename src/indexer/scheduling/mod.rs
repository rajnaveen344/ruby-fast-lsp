//! Server-wide work admission and indexing progress: the CPU/task/memory/I/O
//! governor, the indexing queue, status reporting, and demand-driven navigation
//! indexing.

pub(crate) mod navigation_demand;
pub mod resources;
pub mod scheduler;
pub mod status;

#[cfg(test)]
pub(crate) mod test_schedule;
