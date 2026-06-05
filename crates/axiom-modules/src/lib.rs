//! Module graph construction and file discovery for multi-file Axiom projects.
//!
//! One `.ax` file = one module. The graph is built from the directory structure
//! (design doc `docs/modules-design.md`).

pub mod discover;
pub mod error;
pub mod graph;

#[cfg(test)]
mod tests;
