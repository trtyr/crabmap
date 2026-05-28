// Analyzer module - Rust AST analysis and graph construction
//
// This module is being refactored from a single large file into
// multiple focused sub-modules for better maintainability.

// Re-export the main public API
pub use self::index::index_project;
pub use self::types::IndexOptions;

// Sub-modules
pub mod builder;
mod helpers;
mod index;
pub mod resolution;
mod types;
pub mod visitors;
