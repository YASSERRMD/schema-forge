// Schema-Forge: An intelligent CLI-based database agent
//
// This is the main entry point for the Schema-Forge application.

mod cli;
mod config;
mod database;
mod error;
mod llm;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Schema-Forge v{}", env!("CARGO_PKG_VERSION"));
    println!("Initializing...\n");

    // TODO: Initialize REPL in Phase 4
    println!("CLI REPL coming in Phase 4!");

    Ok(())
}
