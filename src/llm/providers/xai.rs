//! xAI API Provider
//!
//! This module implements the LLMProvider trait for xAI's Grok API.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// xAI API base URL
const XAI_API_BASE: &str = "https://api.x.ai/v1/chat/completions";

/// xAI API provider
pub struct XAIProvider {
    /// API key for authentication
    api_key: String,
    /// Model to use (e.g., "grok-beta")
    model: String,
    /// HTTP client for making requests
    client: LLMHttpClient,
    /// Maximum tokens for generation
    max_tokens: u32,
}

impl XAIProvider {
    /// Create a new xAI provider
    ///
    /// # Arguments
    /// * `api_key` - xAI API key
    /// * `model` - Model identifier (defaults to grok-beta)
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "grok-beta".to_string());
        Self {
            api_key: api_key.into(),
            model,
            client: LLMHttpClient::new().expect("Failed to create HTTP client"),
            max_tokens: 4096,
        }
    }

    /// Set the maximum tokens for generation
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Build headers for xAI API
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        LLMHttpClient::build_headers(&self.api_key)
    }

    /// Convert our Message format to xAI format (OpenAI-compatible)
    fn convert_messages_to_xai(&self, messages: &[Message]) -> Vec<XAIMessage> {
        messages
            .iter()
            .map(|msg| XAIMessage {
                role: match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                }
                .to_string(),
                content: msg.content.clone(),
            })
            .collect()
    }

    /// Extract text content from xAI response
    fn extract_content(&self, response: &XAIResponse) -> String {
        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMProvider for XAIProvider {
    /// Generate a response from the xAI API
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let max_tokens = params
            .and_then(|p| p.max_tokens)
            .unwrap_or(self.max_tokens);

        let temperature: f32 = params.and_then(|p| p.temperature).unwrap_or(0.7);

        let xai_messages = self.convert_messages_to_xai(messages);

        let request = XAIRequest {
            model: self.model.clone(),
            messages: xai_messages,
            max_tokens: Some(max_tokens),
            temperature: Some(temperature),
            top_p: params.and_then(|p| p.top_p),
            stop: params.and_then(|p| p.stop_sequences.clone()),
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(XAI_API_BASE, headers, &request)
            .await?;

        let xai_response: XAIResponse = serde_json::from_str(&response_text).map_err(|e| {
            SchemaForgeError::LLMApiError {
                provider: "xAI".to_string(),
                message: format!("Failed to parse response: {}", e),
                status: 0,
            }
        })?;

        let content = self.extract_content(&xai_response);

        Ok(LLMResponse {
            content,
            model: Some(xai_response.model),
            input_tokens: xai_response.usage.as_ref().map(|u| u.prompt_tokens),
            output_tokens: xai_response.usage.as_ref().map(|u| u.completion_tokens),
            total_tokens: xai_response.usage.as_ref().map(|u| u.total_tokens),
            finish_reason: xai_response.choices.first().and_then(|c| c.finish_reason.clone()),
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

        let messages = vec![
            Message {
                role: MessageRole::System,
                content: format!(
                    "{}\n\nDatabase Schema:\n{}",
                    system_prompt, schema_context
                ),
            },
            Message {
                role: MessageRole::User,
                content: user_query.to_string(),
            },
        ];

        self.generate(&messages, params).await
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

Return only the SQL query with no markdown formatting.";

        let messages = vec![
            Message {
                role: MessageRole::System,
                content: system_prompt.to_string(),
            },
            Message {
                role: MessageRole::User,
                content: format!(
                    "Database Schema:\n{}\n\nQuery: {}",
                    schema_context, natural_language_query
                ),
            },
        ];

        let response = self.generate(&messages, None).await?;
        Ok(response.content.trim().to_string())
    }

    /// Get provider name
    fn provider_name(&self) -> &str {
        "xAI"
    }

    /// Check if API key is set
    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// xAI API request format (OpenAI-compatible)
#[derive(Debug, Serialize)]
struct XAIRequest {
    model: String,
    messages: Vec<XAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

/// xAI API message format
#[derive(Debug, Serialize, Clone)]
struct XAIMessage {
    role: String,
    content: String,
}

/// xAI API response format (OpenAI-compatible)
#[derive(Debug, Deserialize)]
struct XAIResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<XAIChoice>,
    usage: Option<XAIUsage>,
}

/// Choice in xAI response
#[derive(Debug, Deserialize, Clone)]
struct XAIChoice {
    index: u32,
    message: XAIMessageResponse,
    finish_reason: Option<String>,
}

/// Message in xAI response
#[derive(Debug, Deserialize, Clone)]
struct XAIMessageResponse {
    role: String,
    content: Option<String>,
}

/// Token usage information
#[derive(Debug, Deserialize, Clone)]
struct XAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xai_provider_creation() {
        let provider = XAIProvider::new("test-key", None);
        assert_eq!(provider.model, "grok-beta");
        assert_eq!(provider.max_tokens, 4096);
    }

    #[test]
    fn test_xai_provider_with_custom_model() {
        let provider = XAIProvider::new("test-key", Some("grok-2".to_string()));
        assert_eq!(provider.model, "grok-2");
    }

    #[test]
    fn test_xai_provider_with_max_tokens() {
        let provider = XAIProvider::new("test-key", None).with_max_tokens(8192);
        assert_eq!(provider.max_tokens, 8192);
    }

    #[test]
    fn test_has_api_key() {
        let provider = XAIProvider::new("test-key", None);
        assert!(provider.has_api_key());

        let provider = XAIProvider::new("", None);
        assert!(!provider.has_api_key());
    }
}
