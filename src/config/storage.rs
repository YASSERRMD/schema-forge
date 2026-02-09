//! Configuration Storage
//!
//! This module handles persistent storage of configuration data
//! including API keys, model settings, and user preferences.

use crate::error::{Result, SchemaForgeError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Configuration file name
const CONFIG_FILE: &str = "config.toml";

/// Persistent configuration data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// API keys for LLM providers
    pub api_keys: std::collections::HashMap<String, String>,
    /// Model configurations for each provider
    pub models: std::collections::HashMap<String, String>,
    /// Current selected provider
    pub current_provider: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_keys: std::collections::HashMap::new(),
            models: Self::default_models(),
            current_provider: None,
        }
    }
}

impl Config {
    /// Get default models for each provider
    pub fn default_models() -> std::collections::HashMap<String, String> {
        let mut models = std::collections::HashMap::new();

        // Current as of 2025
        models.insert("anthropic".to_string(), "claude-sonnet-4-20250514".to_string());
        models.insert("openai".to_string(), "gpt-4o".to_string());
        models.insert("groq".to_string(), "llama-3.3-70b-versatile".to_string());
        models.insert("cohere".to_string(), "command-r-plus".to_string());
        models.insert("xai".to_string(), "grok-2".to_string());
        models.insert("minimax".to_string(), "abab6.5s-chat".to_string());
        models.insert("qwen".to_string(), "qwen-max".to_string());
        models.insert("zai".to_string(), "deepseek-r1".to_string());

        models
    }

    /// Create a new empty configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the configuration directory path
    pub fn config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| SchemaForgeError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not find configuration directory",
            )))?
            .join("schema-forge");

        // Ensure directory exists
        fs::create_dir_all(&config_dir).map_err(|e| {
            SchemaForgeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create config directory: {}", e),
            ))
        })?;

        Ok(config_dir)
    }

    /// Get the configuration file path
    pub fn config_file() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join(CONFIG_FILE))
    }

    /// Load configuration from disk
    pub fn load() -> Result<Self> {
        let config_file = Self::config_file()?;

        if !config_file.exists() {
            // Return default config if file doesn't exist
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&config_file).map_err(|e| {
            SchemaForgeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read config file: {}", e),
            ))
        })?;

        let config: Config = toml::from_str(&content).map_err(|e| {
            SchemaForgeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to parse config file: {}", e),
            ))
        })?;

        Ok(config)
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<()> {
        let config_file = Self::config_file()?;

        let content = toml::to_string_pretty(self).map_err(|e| {
            SchemaForgeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to serialize config: {}", e),
            ))
        })?;

        fs::write(&config_file, content).map_err(|e| {
            SchemaForgeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to write config file: {}", e),
            ))
        })?;

        Ok(())
    }

    /// Get model for a provider
    pub fn get_model(&self, provider: &str) -> Option<String> {
        self.models.get(provider).cloned()
    }

    /// Set model for a provider
    pub fn set_model(&mut self, provider: String, model: String) {
        self.models.insert(provider, model);
    }

    /// Remove model configuration for a provider (revert to default)
    pub fn remove_model(&mut self, provider: &str) {
        self.models.remove(provider);
    }

    /// Set API key for a provider
    pub fn set_api_key(&mut self, provider: String, key: String) {
        self.api_keys.insert(provider, key);
    }

    /// Get API key for a provider
    pub fn get_api_key(&self, provider: &str) -> Option<&String> {
        self.api_keys.get(provider)
    }

    /// List all configured providers
    pub fn list_providers(&self) -> Vec<String> {
        self.api_keys.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = Config::new();
        assert!(!config.models.is_empty());
        assert!(config.models.contains_key("anthropic"));
        assert!(config.models.contains_key("openai"));
    }

    #[test]
    fn test_model_management() {
        let mut config = Config::new();

        // Set custom model
        config.set_model("anthropic".to_string(), "claude-3-opus-20240229".to_string());
        assert_eq!(
            config.get_model("anthropic"),
            Some("claude-3-opus-20240229".to_string())
        );

        // Remove model - returns None since it's removed from the map
        config.remove_model("anthropic");
        assert_eq!(config.get_model("anthropic"), None);
    }
}
