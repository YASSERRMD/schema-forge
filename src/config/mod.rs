//! Configuration module
//!
//! This module handles configuration management,
//! including API key storage and application settings.

pub mod storage;

use crate::database::manager::DatabaseManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Application state
pub struct AppState {
    /// Database manager (optional - not connected until /connect)
    pub database_manager: Option<DatabaseManager>,
    /// LLM provider API keys
    pub api_keys: HashMap<String, String>,
    /// Current selected provider
    pub current_provider: Option<String>,
}

impl AppState {
    /// Create a new application state
    pub fn new() -> Self {
        Self {
            database_manager: None,
            api_keys: HashMap::new(),
            current_provider: None,
        }
    }

    /// Set the database manager
    pub fn set_database_manager(&mut self, manager: DatabaseManager) {
        self.database_manager = Some(manager);
    }

    /// Store an API key for a provider
    pub fn set_api_key(&mut self, provider: String, key: String) {
        self.api_keys.insert(provider.clone(), key);
        // If this is the first provider, make it the current one
        if self.current_provider.is_none() {
            self.current_provider = Some(provider);
        }
    }

    /// Get API key for a provider
    pub fn get_api_key(&self, provider: &str) -> Option<&String> {
        self.api_keys.get(provider)
    }

    /// Set the current provider
    pub fn set_current_provider(&mut self, provider: String) {
        self.current_provider = Some(provider);
    }

    /// Get the current provider
    pub fn get_current_provider(&self) -> Option<&String> {
        self.current_provider.as_ref()
    }

    /// Check if database is connected
    pub fn is_connected(&self) -> bool {
        self.database_manager.is_some()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared application state
pub type SharedState = Arc<RwLock<AppState>>;

/// Create a new shared state
pub fn create_shared_state() -> SharedState {
    Arc::new(RwLock::new(AppState::new()))
}
