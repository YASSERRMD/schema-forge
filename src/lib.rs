//! Schema-Forge Library
//!
//! This is the library interface for Schema-Forge.
//! The main binary is in src/main.rs.

// Allow dead code for API response fields and trait implementations
#![allow(dead_code)]
#![warn(unused_imports)]

pub mod cli;
pub mod config;
pub mod database;
pub mod error;
pub mod llm;
