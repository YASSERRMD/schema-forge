//! Configuration module
//!
//! This module handles configuration management,
//! including API key storage and application settings.

pub mod storage;

use crate::database::manager::DatabaseManager;
use crate::error::Result;
use crate::llm::provider::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Application state
pub struct AppState {
    /// Database manager (optional - not connected until /connect)
    pub database_manager: Option<DatabaseManager>,
    /// LLM provider API keys
    pub api_keys: HashMap<String, String>,
    /// Model configurations for each provider
    pub models: HashMap<String, String>,
    /// Current selected provider
    pub current_provider: Option<String>,
    /// In-memory multi-turn conversation history for the active session
    pub conversation_history: Vec<Message>,
}

impl AppState {
    /// Create a new application state, loading from disk if available
    pub fn new() -> Self {
        // Try to load from disk, fall back to empty state
        match storage::Config::load() {
            Ok(config) => Self {
                database_manager: None,
                api_keys: config.api_keys,
                models: config.models,
                current_provider: config.current_provider,
                conversation_history: Vec::new(),
            },
            Err(_) => Self {
                database_manager: None,
                api_keys: HashMap::new(),
                models: storage::Config::default_models(),
                current_provider: None,
                conversation_history: Vec::new(),
            },
        }
    }

    /// Set the database manager
    pub fn set_database_manager(&mut self, manager: DatabaseManager) {
        self.database_manager = Some(manager);
        self.clear_conversation_history();
    }

    /// Store an API key for a provider and save to disk
    pub fn set_api_key(&mut self, provider: String, key: String) {
        self.api_keys.insert(provider.clone(), key);
        // Always make the newly configured provider the current one
        self.current_provider = Some(provider.clone());
        // Save to disk
        let _ = self.save();
    }

    /// Get API key for a provider
    pub fn get_api_key(&self, provider: &str) -> Option<&String> {
        self.api_keys.get(provider)
    }

    /// Set model for a provider and save to disk
    pub fn set_model(&mut self, provider: String, model: String) {
        self.models.insert(provider, model);
        // Save to disk
        let _ = self.save();
    }

    /// Get model for a provider
    pub fn get_model(&self, provider: &str) -> Option<String> {
        self.models.get(provider).cloned()
    }

    /// Remove model for a provider (revert to default) and save to disk
    pub fn remove_model(&mut self, provider: &str) {
        self.models.remove(provider);
        // Save to disk
        let _ = self.save();
    }

    /// Set the current provider and save to disk
    pub fn set_current_provider(&mut self, provider: String) {
        self.current_provider = Some(provider);
        // Save to disk
        let _ = self.save();
    }

    /// Get the current provider
    pub fn get_current_provider(&self) -> Option<&String> {
        self.current_provider.as_ref()
    }

    /// Check if database is connected
    pub fn is_connected(&self) -> bool {
        self.database_manager.is_some()
    }

    /// List all configured providers
    pub fn list_providers(&self) -> Vec<String> {
        self.api_keys.keys().cloned().collect()
    }

    /// Get the current in-memory conversation history
    pub fn conversation_history(&self) -> Vec<Message> {
        self.conversation_history.clone()
    }

    /// Append a message to the in-memory conversation history
    pub fn push_conversation_message(&mut self, message: Message) {
        self.conversation_history.push(message);
        if self.conversation_history.len() > 12 {
            let overflow = self.conversation_history.len() - 12;
            self.conversation_history.drain(0..overflow);
        }
    }

    /// Clear the in-memory conversation history
    pub fn clear_conversation_history(&mut self) {
        self.conversation_history.clear();
    }

    /// Save configuration to disk
    fn save(&self) -> Result<()> {
        let config = storage::Config {
            api_keys: self.api_keys.clone(),
            models: self.models.clone(),
            current_provider: self.current_provider.clone(),
        };
        config.save()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared application state
pub type SharedState = Arc<RwLock<AppState>>;

/// Create a new shared state, loading from disk if available
pub fn create_shared_state() -> SharedState {
    Arc::new(RwLock::new(AppState::new()))
}
