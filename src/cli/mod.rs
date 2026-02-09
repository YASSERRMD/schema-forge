//! CLI module
//!
//! This module provides the command-line interface for Schema-Forge,
//! including the REPL implementation and command handlers.

pub mod command_menu;
pub mod commands;
pub mod repl;

// Re-exports
pub use repl::Repl;
