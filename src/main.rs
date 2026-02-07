// Schema-Forge: An intelligent CLI-based database agent
//
// This is the main entry point for the Schema-Forge application.

mod cli;
mod config;
mod database;
mod error;
mod llm;

use cli::Repl;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create and run the REPL
    let mut repl = Repl::new()?;
    repl.run().await?;

    Ok(())
}
