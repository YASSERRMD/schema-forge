// Schema-Forge: An intelligent CLI-based database agent
//
// This is the main entry point for the Schema-Forge application.

mod cli;
mod config;
mod database;
mod error;
mod llm;

use cli::Repl;
use config::create_shared_state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create shared application state
    let state = create_shared_state();

    // Create and run the REPL
    let mut repl = Repl::new(state)?;
    repl.run().await?;

    Ok(())
}
