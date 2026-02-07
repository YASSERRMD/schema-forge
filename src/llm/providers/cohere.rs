//! Cohere API Provider
//!
//! This module implements the LLMProvider trait for Cohere's API.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Cohere API base URL
const COHERE_API_BASE: &str = "https://api.cohere.ai/v1/chat";

/// Cohere API provider
pub struct CohereProvider {
    /// API key for authentication
    api_key: String,
    /// Model to use (e.g., "command-r-plus")
    model: String,
    /// HTTP client for making requests
    client: LLMHttpClient,
}

impl CohereProvider {
    /// Create a new Cohere provider
    ///
    /// # Arguments
    /// * `api_key` - Cohere API key
    /// * `model` - Model identifier (defaults to command-r-plus)
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "command-r-plus".to_string());
        Self {
            api_key: api_key.into(),
            model,
            client: LLMHttpClient::new().expect("Failed to create HTTP client"),
        }
    }

    /// Build headers for Cohere API
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        LLMHttpClient::build_headers_with_auth("Authorization", &format!("Bearer {}", self.api_key))
    }

    /// Convert our Message format to Cohere format
    fn convert_messages_to_cohere(&self, messages: &[Message]) -> String {
        // Cohere uses a single chat_history with role/message format
        // Build a conversation string
        messages
            .iter()
            .map(|msg| match msg.role {
                MessageRole::User => format!("User: {}", msg.content),
                MessageRole::Assistant => format!("Chatbot: {}", msg.content),
                MessageRole::System => format!("System: {}", msg.content),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[async_trait]
impl LLMProvider for CohereProvider {
    /// Generate a response from the Cohere API
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        // Get the last message as the main query
        let last_message = messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Build chat_history from previous messages
        let chat_history: Vec<CohereChatMessage> = messages
            .iter()
            .take(messages.len() - 1)
            .filter_map(|msg| {
                let role = match msg.role {
                    MessageRole::User => "USER",
                    MessageRole::Assistant => "CHATBOT",
                    MessageRole::System => return None, // Skip system messages in history
                };
                Some(CohereChatMessage {
                    role: role.to_string(),
                    message: msg.content.clone(),
                })
            })
            .collect();

        let temperature: f32 = params.and_then(|p| p.temperature).unwrap_or(0.7);

        let request = CohereRequest {
            message: last_message,
            chat_history: if chat_history.is_empty() { None } else { Some(chat_history) },
            model: self.model.clone(),
            temperature: Some(temperature),
            max_tokens: params.and_then(|p| p.max_tokens),
            p: params.and_then(|p| p.top_p),
            stop_sequences: params.and_then(|p| p.stop_sequences.clone()),
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(COHERE_API_BASE, headers, &request)
            .await?;

        let cohere_response: CohereResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "Cohere".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        Ok(LLMResponse {
            content: cohere_response.text,
            model: Some(cohere_response.response_id), // Cohere doesn't return model name
            input_tokens: Some(cohere_response.meta.tokens.input_tokens),
            output_tokens: Some(cohere_response.meta.tokens.output_tokens),
            total_tokens: Some(
                cohere_response.meta.tokens.input_tokens + cohere_response.meta.tokens.output_tokens,
            ),
            finish_reason: cohere_response.finish_reason,
        })
    }

    /// Generate a response with schema context
    async fn generate_with_schema(
        &self,
        schema_context: &str,
        user_query: &str,
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let preamble = "You are a database expert. Answer questions about database schemas based on the provided context.";

        let chat_history = vec![CohereChatMessage {
            role: "SYSTEM".to_string(),
            message: format!("{}\n\nDatabase Schema:\n{}", preamble, schema_context),
        }];

        let temperature: f32 = params.and_then(|p| p.temperature).unwrap_or(0.7);

        let request = CohereRequest {
            message: user_query.to_string(),
            chat_history: Some(chat_history),
            model: self.model.clone(),
            temperature: Some(temperature),
            max_tokens: params.and_then(|p| p.max_tokens),
            p: params.and_then(|p| p.top_p),
            stop_sequences: params.and_then(|p| p.stop_sequences.clone()),
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(COHERE_API_BASE, headers, &request)
            .await?;

        let cohere_response: CohereResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "Cohere".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        Ok(LLMResponse {
            content: cohere_response.text,
            model: Some(cohere_response.response_id),
            input_tokens: Some(cohere_response.meta.tokens.input_tokens),
            output_tokens: Some(cohere_response.meta.tokens.output_tokens),
            total_tokens: Some(
                cohere_response.meta.tokens.input_tokens + cohere_response.meta.tokens.output_tokens,
            ),
            finish_reason: cohere_response.finish_reason,
        })
    }

    /// Generate SQL from natural language
    async fn generate_sql(
        &self,
        schema_context: &str,
        natural_language_query: &str,
    ) -> Result<String> {
        let preamble = "You are a SQL expert. Convert natural language queries to SQL based on the provided database schema.

Rules:
1. Return ONLY the SQL query, no explanations
2. Use proper table and column names from the schema
3. Handle NULL values appropriately
4. Use proper JOIN syntax
5. Add appropriate WHERE clauses
6. Format SQL in a readable way

Return only the SQL query with no markdown formatting.";

        let chat_history = vec![CohereChatMessage {
            role: "SYSTEM".to_string(),
            message: format!("{}\n\nDatabase Schema:\n{}", preamble, schema_context),
        }];

        let request = CohereRequest {
            message: natural_language_query.to_string(),
            chat_history: Some(chat_history),
            model: self.model.clone(),
            temperature: Some(0.3), // Lower temperature for SQL
            max_tokens: Some(2048),
            p: None,
            stop_sequences: None,
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(COHERE_API_BASE, headers, &request)
            .await?;

        let cohere_response: CohereResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "Cohere".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        Ok(cohere_response.text.trim().to_string())
    }

    /// Get provider name
    fn provider_name(&self) -> &str {
        "Cohere"
    }

    /// Check if API key is set
    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// Cohere API request format
#[derive(Debug, Serialize)]
struct CohereRequest {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_history: Option<Vec<CohereChatMessage>>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(rename = "p")]
    #[serde(skip_serializing_if = "Option::is_none")]
    p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
}

/// Cohere chat message
#[derive(Debug, Serialize, Clone)]
struct CohereChatMessage {
    role: String,
    message: String,
}

/// Cohere API response format
#[derive(Debug, Deserialize)]
struct CohereResponse {
    text: String,
    response_id: String,
    finish_reason: Option<String>,
    meta: CohereMeta,
}

/// Cohere metadata
#[derive(Debug, Deserialize, Clone)]
struct CohereMeta {
    tokens: CohereTokens,
}

/// Cohere token usage
#[derive(Debug, Deserialize, Clone)]
struct CohereTokens {
    input_tokens: u32,
    output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cohere_provider_creation() {
        let provider = CohereProvider::new("test-key", None);
        assert_eq!(provider.model, "command-r-plus");
    }

    #[test]
    fn test_cohere_provider_with_custom_model() {
        let provider = CohereProvider::new("test-key", Some("command-r".to_string()));
        assert_eq!(provider.model, "command-r");
    }

    #[test]
    fn test_has_api_key() {
        let provider = CohereProvider::new("test-key", None);
        assert!(provider.has_api_key());

        let provider = CohereProvider::new("", None);
        assert!(!provider.has_api_key());
    }
}
