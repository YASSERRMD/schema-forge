//! z.ai API Provider
//!
//! This module implements the LLMProvider trait for z.ai's API.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// z.ai API base URL
const ZAI_API_BASE: &str = "https://api.z.ai/v1/chat/completions";

/// z.ai API provider
pub struct ZAIProvider {
    /// API key for authentication
    api_key: String,
    /// Model to use
    model: String,
    /// HTTP client for making requests
    client: LLMHttpClient,
    /// Maximum tokens for generation
    max_tokens: u32,
}

impl ZAIProvider {
    /// Create a new z.ai provider
    ///
    /// # Arguments
    /// * `api_key` - z.ai API key
    /// * `model` - Model identifier (defaults to z-pro-v1)
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "z-pro-v1".to_string());
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

    /// Build headers for z.ai API
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        LLMHttpClient::build_headers(&self.api_key)
    }

    /// Convert our Message format to z.ai format (OpenAI-compatible)
    fn convert_messages_to_zai(&self, messages: &[Message]) -> Vec<ZAIMessage> {
        messages
            .iter()
            .map(|msg| ZAIMessage {
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

    /// Extract text content from z.ai response
    fn extract_content(&self, response: &ZAIResponse) -> String {
        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMProvider for ZAIProvider {
    /// Generate a response from the z.ai API
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let max_tokens = params
            .and_then(|p| p.max_tokens)
            .unwrap_or(self.max_tokens);

        let temperature: f32 = params.and_then(|p| p.temperature).unwrap_or(0.7);

        let zai_messages = self.convert_messages_to_zai(messages);

        let request = ZAIRequest {
            model: self.model.clone(),
            messages: zai_messages,
            max_tokens: Some(max_tokens),
            temperature: Some(temperature),
            top_p: params.and_then(|p| p.top_p),
            stop: params.and_then(|p| p.stop_sequences.clone()),
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(ZAI_API_BASE, headers, &request)
            .await?;

        let zai_response: ZAIResponse = serde_json::from_str(&response_text).map_err(|e| {
            SchemaForgeError::LLMApiError {
                provider: "z.ai".to_string(),
                message: format!("Failed to parse response: {}", e),
                status: 0,
            }
        })?;

        let content = self.extract_content(&zai_response);

        Ok(LLMResponse {
            content,
            model: Some(zai_response.model),
            input_tokens: zai_response.usage.as_ref().map(|u| u.prompt_tokens),
            output_tokens: zai_response.usage.as_ref().map(|u| u.completion_tokens),
            total_tokens: zai_response.usage.as_ref().map(|u| u.total_tokens),
            finish_reason: zai_response.choices.first().and_then(|c| c.finish_reason.clone()),
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
        "z.ai"
    }

    /// Check if API key is set
    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// z.ai API request format (OpenAI-compatible)
#[derive(Debug, Serialize)]
struct ZAIRequest {
    model: String,
    messages: Vec<ZAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

/// z.ai API message format
#[derive(Debug, Serialize, Clone)]
struct ZAIMessage {
    role: String,
    content: String,
}

/// z.ai API response format (OpenAI-compatible)
#[derive(Debug, Deserialize)]
struct ZAIResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ZAIChoice>,
    usage: Option<ZAIUsage>,
}

/// Choice in z.ai response
#[derive(Debug, Deserialize, Clone)]
struct ZAIChoice {
    index: u32,
    message: ZAIMessageResponse,
    finish_reason: Option<String>,
}

/// Message in z.ai response
#[derive(Debug, Deserialize, Clone)]
struct ZAIMessageResponse {
    role: String,
    content: Option<String>,
}

/// Token usage information
#[derive(Debug, Deserialize, Clone)]
struct ZAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zai_provider_creation() {
        let provider = ZAIProvider::new("test-key", None);
        assert_eq!(provider.model, "z-pro-v1");
        assert_eq!(provider.max_tokens, 4096);
    }

    #[test]
    fn test_zai_provider_with_custom_model() {
        let provider = ZAIProvider::new("test-key", Some("z-ultra-v2".to_string()));
        assert_eq!(provider.model, "z-ultra-v2");
    }

    #[test]
    fn test_zai_provider_with_max_tokens() {
        let provider = ZAIProvider::new("test-key", None).with_max_tokens(8192);
        assert_eq!(provider.max_tokens, 8192);
    }

    #[test]
    fn test_has_api_key() {
        let provider = ZAIProvider::new("test-key", None);
        assert!(provider.has_api_key());

        let provider = ZAIProvider::new("", None);
        assert!(!provider.has_api_key());
    }
}
