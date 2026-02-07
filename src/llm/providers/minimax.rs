//! Minimax API Provider
//!
//! This module implements the LLMProvider trait for Minimax's API.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Minimax API base URL
const MINIMAX_API_BASE: &str = "https://api.minimax.chat/v1/text/chatcompletion_v2";

/// Minimax API provider
pub struct MinimaxProvider {
    /// API key for authentication
    api_key: String,
    /// Model to use
    model: String,
    /// HTTP client for making requests
    client: LLMHttpClient,
}

impl MinimaxProvider {
    /// Create a new Minimax provider
    ///
    /// # Arguments
    /// * `api_key` - Minimax API key
    /// * `model` - Model identifier (defaults to abab6.5s-chat)
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "abab6.5s-chat".to_string());
        Self {
            api_key: api_key.into(),
            model,
            client: LLMHttpClient::new().expect("Failed to create HTTP client"),
        }
    }

    /// Build headers for Minimax API
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        LLMHttpClient::build_headers(&self.api_key)
    }

    /// Convert our Message format to Minimax format
    fn convert_messages_to_minimax(&self, messages: &[Message]) -> Vec<MinimaxMessage> {
        messages
            .iter()
            .map(|msg| MinimaxMessage {
                role: match msg.role {
                    MessageRole::User => "USER",
                    MessageRole::Assistant => "BOT",
                    MessageRole::System => "SYSTEM",
                }
                .to_string(),
                text: msg.content.clone(),
                name: None,
            })
            .collect()
    }
}

#[async_trait]
impl LLMProvider for MinimaxProvider {
    /// Generate a response from the Minimax API
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let temperature: f32 = params.and_then(|p| p.temperature).unwrap_or(0.7);

        let minimax_messages = self.convert_messages_to_minimax(messages);

        let request = MinimaxRequest {
            model: self.model.clone(),
            messages: minimax_messages,
            temperature: Some(temperature),
            top_p: params.and_then(|p| p.top_p),
            max_tokens: params.and_then(|p| p.max_tokens),
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(MINIMAX_API_BASE, headers, &request)
            .await?;

        let minimax_response: MinimaxResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "Minimax".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        let content = minimax_response
            .choices
            .first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default();

        Ok(LLMResponse {
            content,
            model: Some(self.model.clone()),
            input_tokens: Some(minimax_response.usage.input_tokens),
            output_tokens: Some(minimax_response.usage.output_tokens),
            total_tokens: Some(
                minimax_response.usage.input_tokens + minimax_response.usage.output_tokens,
            ),
            finish_reason: None,
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
        "Minimax"
    }

    /// Check if API key is set
    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// Minimax API request format
#[derive(Debug, Serialize)]
struct MinimaxRequest {
    model: String,
    messages: Vec<MinimaxMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// Minimax API message format
#[derive(Debug, Serialize, Clone)]
struct MinimaxMessage {
    role: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Minimax API response format
#[derive(Debug, Deserialize)]
struct MinimaxResponse {
    base_resp: MinimaxBaseResp,
    choices: Vec<MinimaxChoice>,
    usage: MinimaxUsage,
}

/// Minimax base response
#[derive(Debug, Deserialize)]
struct MinimaxBaseResp {
    status_code: u32,
    status_msg: String,
}

/// Minimax choice
#[derive(Debug, Deserialize, Clone)]
struct MinimaxChoice {
    text: Option<String>,
}

/// Minimax token usage
#[derive(Debug, Deserialize, Clone)]
struct MinimaxUsage {
    total_tokens: u32,
    input_tokens: u32,
    output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimax_provider_creation() {
        let provider = MinimaxProvider::new("test-key", None);
        assert_eq!(provider.model, "abab6.5s-chat");
    }

    #[test]
    fn test_minimax_provider_with_custom_model() {
        let provider = MinimaxProvider::new("test-key", Some("abab5.5-chat".to_string()));
        assert_eq!(provider.model, "abab5.5-chat");
    }

    #[test]
    fn test_has_api_key() {
        let provider = MinimaxProvider::new("test-key", None);
        assert!(provider.has_api_key());

        let provider = MinimaxProvider::new("", None);
        assert!(!provider.has_api_key());
    }
}
