//! REPL implementation.
//!
//! The shell now delegates to a persistent TUI so the banner remains fixed
//! instead of scrolling away with command output.

use crate::config::SharedState;
use crate::error::Result;

/// Schema-Forge REPL
pub struct Repl {
    /// Whether the REPL should continue running
    running: bool,
    /// Shared application state
    state: SharedState,
}

impl Repl {
    /// Create a new REPL instance
    pub fn new(state: SharedState) -> Result<Self> {
        Ok(Self {
            running: true,
            state,
        })
    }

    /// Run the REPL loop
    pub async fn run(&mut self) -> Result<()> {
        let mut app = crate::cli::tui::TuiApp::new(self.state.clone());
        app.run().await?;
        self.running = false;
        Ok(())
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new(crate::config::create_shared_state()).expect("Failed to create REPL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_creation() {
        let state = crate::config::create_shared_state();
        let repl = Repl::new(state);
        assert!(repl.is_ok());
        let repl = repl.unwrap();
        assert!(repl.running);
    }
}
