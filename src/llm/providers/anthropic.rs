//! Anthropic Claude API Provider
//!
//! This module implements the LLMProvider trait for Anthropic's Claude API.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

/// Anthropic API base URL
const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com/v1/messages";

/// Anthropic Claude API provider
pub struct AnthropicProvider {
    /// API key for authentication
    api_key: String,
    /// Model to use (e.g., "claude-3-5-sonnet-20241022")
    model: String,
    /// HTTP client for making requests
    client: LLMHttpClient,
    /// API version
    version: String,
    /// Maximum tokens for generation
    max_tokens: u32,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    ///
    /// # Arguments
    /// * `api_key` - Anthropic API key
    /// * `model` - Model identifier (defaults to claude-3-5-sonnet-20241022)
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
        Self {
            api_key: api_key.into(),
            model,
            client: LLMHttpClient::new().expect("Failed to create HTTP client"),
            version: "2023-06-01".to_string(),
            max_tokens: 4096,
        }
    }

    /// Set the maximum tokens for generation
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set the API version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Build headers for Anthropic API
    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        headers.insert("x-api-key", self.api_key.parse().unwrap());
        headers.insert("anthropic-version", self.version.parse().unwrap());
        headers
    }

    /// Convert our Message format to Anthropic format
    fn convert_messages_to_anthropic(&self, messages: &[Message]) -> Vec<AnthropicMessage> {
        messages
            .iter()
            .map(|msg| AnthropicMessage {
                role: match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "user", // Anthropic doesn't have system role in messages
                }
                .to_string(),
                content: msg.content.clone(),
            })
            .collect()
    }

    /// Extract text content from Anthropic response
    fn extract_content(&self, response: &AnthropicResponse) -> String {
        if response.content.is_empty() {
            return String::new();
        }

        // Concatenate all text blocks
        response
            .content
            .iter()
            .filter_map(|block| {
                if block.type_ == "text" {
                    Some(block.text.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    /// Generate a response from the Claude API
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let max_tokens = params
            .and_then(|p| p.max_tokens)
            .unwrap_or(self.max_tokens);

        let temperature: f64 = params.and_then(|p| p.temperature).unwrap_or(0.7) as f64;

        let anthropic_messages = self.convert_messages_to_anthropic(messages);

        let request = AnthropicRequest {
            model: self.model.clone(),
            messages: anthropic_messages,
            max_tokens,
            temperature: Some(temperature),
            top_p: params.and_then(|p| p.top_p.map(|t| t as f64)),
            stop_sequences: params
                .and_then(|p| p.stop_sequences.clone())
                .unwrap_or_default(),
            system: None, // System messages are handled in the messages array
            stream: false,
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(ANTHROPIC_API_BASE, headers, &request)
            .await?;

        let anthropic_response: AnthropicResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "Anthropic".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        let content = self.extract_content(&anthropic_response);

        Ok(LLMResponse {
            content,
            model: Some(anthropic_response.model),
            input_tokens: Some(anthropic_response.usage.input_tokens),
            output_tokens: Some(anthropic_response.usage.output_tokens),
            total_tokens: Some(anthropic_response.usage.input_tokens + anthropic_response.usage.output_tokens),
            finish_reason: anthropic_response.stop_reason,
        })
    }

    /// Generate a response with schema context
    async fn generate_with_schema(
        &self,
        schema_context: &str,
        user_query: &str,
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let system_prompt = "You are a database expert. Answer questions about database schemas based on the provided context.";

        // Build system prompt with schema context
        let system_with_schema = format!("{}\n\nDatabase Schema:\n{}", system_prompt, schema_context);

        // Create params with system prompt for Anthropic
        let request = AnthropicRequest {
            model: self.model.clone(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: user_query.to_string(),
            }],
            max_tokens: params
                .and_then(|p| p.max_tokens)
                .unwrap_or(self.max_tokens),
            temperature: params.and_then(|p| p.temperature.map(|t| t as f64)),
            top_p: params.and_then(|p| p.top_p.map(|t| t as f64)),
            stop_sequences: params
                .and_then(|p| p.stop_sequences.clone())
                .unwrap_or_default(),
            system: Some(system_with_schema),
            stream: false,
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(ANTHROPIC_API_BASE, headers, &request)
            .await?;

        let anthropic_response: AnthropicResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "Anthropic".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        let content = self.extract_content(&anthropic_response);

        Ok(LLMResponse {
            content,
            model: Some(anthropic_response.model),
            input_tokens: Some(anthropic_response.usage.input_tokens),
            output_tokens: Some(anthropic_response.usage.output_tokens),
            total_tokens: Some(anthropic_response.usage.input_tokens + anthropic_response.usage.output_tokens),
            finish_reason: anthropic_response.stop_reason,
        })
    }

    /// Generate SQL from natural language
    async fn generate_sql(
        &self,
        schema_context: &str,
        natural_language_query: &str,
    ) -> Result<String> {
        let system_prompt = "You are a SQL expert. Convert natural language queries to SQL based on the provided database schema.

Rules:
1. Return ONLY the SQL query, no explanations
2. Use proper table and column names from the schema
3. Handle NULL values appropriately
4. Use proper JOIN syntax
5. Add appropriate WHERE clauses
6. Format SQL in a readable way
7. For PostgreSQL, use ::text for type casting
8. For MySQL, use CAST for type casting
9. For SQLite, use CAST for type casting
10. For MSSQL, use CAST for type casting

Return only the SQL query with no markdown formatting.";

        let system_with_schema = format!("{}\n\nDatabase Schema:\n{}", system_prompt, schema_context);

        let request = AnthropicRequest {
            model: self.model.clone(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: natural_language_query.to_string(),
            }],
            max_tokens: self.max_tokens,
            temperature: Some(0.3), // Lower temperature for SQL generation
            top_p: None,
            stop_sequences: Vec::new(),
            system: Some(system_with_schema),
            stream: false,
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(ANTHROPIC_API_BASE, headers, &request)
            .await?;

        let anthropic_response: AnthropicResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "Anthropic".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        let content = self.extract_content(&anthropic_response);
        Ok(content.trim().to_string())
    }

    /// Get provider name
    fn provider_name(&self) -> &str {
        "Anthropic"
    }

    /// Check if API key is set
    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// Anthropic API request format
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    stream: bool,
}

/// Anthropic API message format
#[derive(Debug, Serialize, Clone)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic API response format
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    role: String,
    content: Vec<ContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: Usage,
}

/// Content block in Anthropic response
#[derive(Debug, Deserialize, Clone)]
struct ContentBlock {
    #[serde(rename = "type")]
    type_: String,
    text: String,
}

/// Token usage information
#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = AnthropicProvider::new("test-key", None);
        assert_eq!(provider.model, "claude-3-5-sonnet-20241022");
        assert_eq!(provider.max_tokens, 4096);
    }

    #[test]
    fn test_anthropic_provider_with_custom_model() {
        let provider = AnthropicProvider::new("test-key", Some("claude-3-opus".to_string()));
        assert_eq!(provider.model, "claude-3-opus");
    }

    #[test]
    fn test_anthropic_provider_with_max_tokens() {
        let provider = AnthropicProvider::new("test-key", None).with_max_tokens(8192);
        assert_eq!(provider.max_tokens, 8192);
    }

    #[test]
    fn test_anthropic_provider_with_version() {
        let provider =
            AnthropicProvider::new("test-key", None).with_version("2023-06-01");
        assert_eq!(provider.version, "2023-06-01");
    }

    #[test]
    fn test_message_conversion() {
        let provider = AnthropicProvider::new("test-key", None);

        let messages = vec![
            Message {
                role: MessageRole::User,
                content: "Hello".to_string(),
            },
            Message {
                role: MessageRole::Assistant,
                content: "Hi there!".to_string(),
            },
        ];

        let anthropic_messages = provider.convert_messages_to_anthropic(&messages);
        assert_eq!(anthropic_messages.len(), 2);
        assert_eq!(anthropic_messages[0].role, "user");
        assert_eq!(anthropic_messages[0].content, "Hello");
        assert_eq!(anthropic_messages[1].role, "assistant");
        assert_eq!(anthropic_messages[1].content, "Hi there!");
    }

    #[test]
    fn test_has_api_key() {
        let provider = AnthropicProvider::new("test-key", None);
        assert!(provider.has_api_key());

        let provider = AnthropicProvider::new("", None);
        assert!(!provider.has_api_key());
    }
}
