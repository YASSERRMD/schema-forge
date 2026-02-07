//! LLM Provider Trait
//!
//! This module defines the trait-based abstraction for LLM providers,
//! enabling easy integration of multiple AI services (Anthropic, OpenAI, etc.)

use crate::error::{Result, SchemaForgeError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// LLM message role
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    /// System message (sets behavior/context)
    System,
    /// User message (query or input)
    User,
    /// Assistant message (response)
    Assistant,
}

/// LLM message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
}

impl Message {
    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
        }
    }

    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
        }
    }
}

/// LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    /// Generated text content
    pub content: String,
    /// Number of tokens used (input)
    pub input_tokens: Option<u32>,
    /// Number of tokens used (output)
    pub output_tokens: Option<u32>,
    /// Total tokens used
    pub total_tokens: Option<u32>,
    /// Model used for generation
    pub model: Option<String>,
    /// Finish reason (e.g., "stop", "length")
    pub finish_reason: Option<String>,
}

impl LLMResponse {
    /// Create a new response
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            model: None,
            finish_reason: None,
        }
    }

    /// Get total token count if available
    pub fn get_total_tokens(&self) -> Option<u32> {
        self.total_tokens
            .or_else(|| {
                self.input_tokens
                    .and_then(|input| self.output_tokens.map(|output| input + output))
            })
    }
}

/// LLM generation parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature (0.0 - 1.0, higher = more random)
    pub temperature: Option<f32>,
    /// Top-p sampling (0.0 - 1.0)
    pub top_p: Option<f32>,
    /// Top-k sampling
    pub top_k: Option<u32>,
    /// Stop sequences
    pub stop_sequences: Option<Vec<String>>,
    /// Presence penalty (0.0 - 2.0)
    pub presence_penalty: Option<f32>,
    /// Frequency penalty (0.0 - 2.0)
    pub frequency_penalty: Option<f32>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            max_tokens: Some(4096),
            temperature: Some(0.7),
            top_p: Some(1.0),
            top_k: None,
            stop_sequences: None,
            presence_penalty: Some(0.0),
            frequency_penalty: Some(0.0),
        }
    }
}

impl GenerationParams {
    /// Create new default parameters
    pub fn new() -> Self {
        Self::default()
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

/// Trait for LLM providers
///
/// This trait defines the interface that all LLM providers must implement,
/// enabling easy addition of new AI services.
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Generate a response from the LLM
    ///
    /// # Arguments
    /// * `messages` - Conversation history
    /// * `params` - Generation parameters
    ///
    /// # Returns
    /// The LLM response
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse>;

    /// Generate a response with schema context
    ///
    /// This is a convenience method that formats the schema context
    /// and user query into appropriate messages for the LLM.
    ///
    /// # Arguments
    /// * `schema_context` - Database schema information
    /// * `user_query` - Natural language query from user
    /// * `params` - Optional generation parameters
    ///
    /// # Returns
    /// The LLM response
    async fn generate_with_schema(
        &self,
        schema_context: &str,
        user_query: &str,
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        // Build system prompt with schema context
        let system_prompt = format!(
            "You are a SQL expert. Given the following database schema, \
            generate SQL queries to answer the user's questions.\n\n\
            Database Schema:\n{}\n\n\
            Only respond with the SQL query. No explanations.",
            schema_context
        );

        let messages = vec![
            Message::system(system_prompt),
            Message::user(user_query),
        ];

        self.generate(&messages, params).await
    }

    /// Generate SQL from natural language
    ///
    /// # Arguments
    /// * `schema_context` - Database schema information
    /// * `natural_language_query` - User's natural language question
    ///
    /// # Returns
    /// Generated SQL query
    async fn generate_sql(
        &self,
        schema_context: &str,
        natural_language_query: &str,
    ) -> Result<String> {
        let response = self
            .generate_with_schema(schema_context, natural_language_query, None)
            .await?;
        Ok(response.content)
    }

    /// Get the provider name
    fn provider_name(&self) -> &str;

    /// Check if the provider has an API key configured
    fn has_api_key(&self) -> bool;

    /// Validate the provider configuration
    fn validate_config(&self) -> Result<()> {
        if !self.has_api_key() {
            return Err(SchemaForgeError::LLMApiKeyMissing(
                self.provider_name().to_string(),
            ));
        }
        Ok(())
    }
}

/// Builder for creating LLM providers
pub struct LLMProviderBuilder {
    /// API key for the provider
    api_key: Option<String>,
    /// Base URL for API requests (for custom endpoints)
    base_url: Option<String>,
    /// Model to use
    model: Option<String>,
    /// Timeout for requests (in seconds)
    timeout: u64,
}

impl Default for LLMProviderBuilder {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            model: None,
            timeout: 60,
        }
    }
}

impl LLMProviderBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the API key
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Set the base URL
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Set the model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }

    /// Get the API key
    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Get the base URL
    pub fn get_base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// Get the model
    pub fn get_model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Get the timeout
    pub fn get_timeout(&self) -> u64 {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let system_msg = Message::system("You are a helpful assistant");
        assert_eq!(system_msg.role, MessageRole::System);

        let user_msg = Message::user("Hello");
        assert_eq!(user_msg.role, MessageRole::User);

        let assistant_msg = Message::assistant("Hi there!");
        assert_eq!(assistant_msg.role, MessageRole::Assistant);
    }

    #[test]
    fn test_generation_params_default() {
        let params = GenerationParams::new();
        assert_eq!(params.max_tokens, Some(4096));
        assert_eq!(params.temperature, Some(0.7));
    }

    #[test]
    fn test_generation_params_builder() {
        let params = GenerationParams::new()
            .with_max_tokens(2048)
            .with_temperature(0.5);

        assert_eq!(params.max_tokens, Some(2048));
        assert_eq!(params.temperature, Some(0.5));
    }

    #[test]
    fn test_llm_response() {
        let response = LLMResponse::new("SELECT * FROM users;");
        assert_eq!(response.content, "SELECT * FROM users;");

        let response_with_tokens = LLMResponse {
            content: "Test".to_string(),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: None,
            model: None,
            finish_reason: None,
        };

        assert_eq!(response_with_tokens.get_total_tokens(), Some(15));
    }

    #[test]
    fn test_provider_builder() {
        let builder = LLMProviderBuilder::new()
            .with_api_key("test-key")
            .with_model("gpt-4")
            .with_timeout(30);

        assert_eq!(builder.get_api_key(), Some("test-key"));
        assert_eq!(builder.get_model(), Some("gpt-4"));
        assert_eq!(builder.get_timeout(), 30);
    }
}
