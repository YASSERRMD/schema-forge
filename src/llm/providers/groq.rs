//! Groq API Provider
//!
//! This module implements the LLMProvider trait for Groq's fast inference API.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Groq API base URL
const GROQ_API_BASE: &str = "https://api.groq.com/openai/v1/chat/completions";

/// Groq API provider
pub struct GroqProvider {
    /// API key for authentication
    api_key: String,
    /// Model to use (e.g., "llama3-70b-8192")
    model: String,
    /// HTTP client for making requests
    client: LLMHttpClient,
    /// Maximum tokens for generation
    max_tokens: u32,
}

impl GroqProvider {
    /// Create a new Groq provider
    ///
    /// # Arguments
    /// * `api_key` - Groq API key
    /// * `model` - Model identifier (defaults to llama3-70b-8192)
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "llama3-70b-8192".to_string());
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

    /// Build headers for Groq API
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        LLMHttpClient::build_headers(&self.api_key)
    }

    /// Convert our Message format to Groq format (OpenAI-compatible)
    fn convert_messages_to_groq(&self, messages: &[Message]) -> Vec<GroqMessage> {
        messages
            .iter()
            .map(|msg| GroqMessage {
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

    /// Extract text content from Groq response
    fn extract_content(&self, response: &GroqResponse) -> String {
        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMProvider for GroqProvider {
    /// Generate a response from the Groq API
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let max_tokens = params
            .and_then(|p| p.max_tokens)
            .unwrap_or(self.max_tokens);

        let temperature: f32 = params.and_then(|p| p.temperature).unwrap_or(0.7);

        let groq_messages = self.convert_messages_to_groq(messages);

        let request = GroqRequest {
            model: self.model.clone(),
            messages: groq_messages,
            max_tokens: Some(max_tokens),
            temperature: Some(temperature),
            top_p: params.and_then(|p| p.top_p),
            stop: params.and_then(|p| p.stop_sequences.clone()),
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(GROQ_API_BASE, headers, &request)
            .await?;

        let groq_response: GroqResponse = serde_json::from_str(&response_text).map_err(|e| {
            SchemaForgeError::LLMApiError {
                provider: "Groq".to_string(),
                message: format!("Failed to parse response: {}", e),
                status: 0,
            }
        })?;

        let content = self.extract_content(&groq_response);

        Ok(LLMResponse {
            content,
            model: Some(groq_response.model),
            input_tokens: groq_response.usage.as_ref().map(|u| u.prompt_tokens),
            output_tokens: groq_response.usage.as_ref().map(|u| u.completion_tokens),
            total_tokens: groq_response.usage.as_ref().map(|u| u.total_tokens),
            finish_reason: groq_response.choices.first().and_then(|c| c.finish_reason.clone()),
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
        "Groq"
    }

    /// Check if API key is set
    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// Groq API request format (OpenAI-compatible)
#[derive(Debug, Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<GroqMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

/// Groq API message format
#[derive(Debug, Serialize, Clone)]
struct GroqMessage {
    role: String,
    content: String,
}

/// Groq API response format (OpenAI-compatible)
#[derive(Debug, Deserialize)]
struct GroqResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<GroqChoice>,
    usage: Option<GroqUsage>,
}

/// Choice in Groq response
#[derive(Debug, Deserialize, Clone)]
struct GroqChoice {
    index: u32,
    message: GroqMessageResponse,
    finish_reason: Option<String>,
}

/// Message in Groq response
#[derive(Debug, Deserialize, Clone)]
struct GroqMessageResponse {
    role: String,
    content: Option<String>,
}

/// Token usage information
#[derive(Debug, Deserialize, Clone)]
struct GroqUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_groq_provider_creation() {
        let provider = GroqProvider::new("test-key", None);
        assert_eq!(provider.model, "llama3-70b-8192");
        assert_eq!(provider.max_tokens, 4096);
    }

    #[test]
    fn test_groq_provider_with_custom_model() {
        let provider = GroqProvider::new("test-key", Some("mixtral-8x7b-32768".to_string()));
        assert_eq!(provider.model, "mixtral-8x7b-32768");
    }

    #[test]
    fn test_groq_provider_with_max_tokens() {
        let provider = GroqProvider::new("test-key", None).with_max_tokens(8192);
        assert_eq!(provider.max_tokens, 8192);
    }

    #[test]
    fn test_has_api_key() {
        let provider = GroqProvider::new("test-key", None);
        assert!(provider.has_api_key());

        let provider = GroqProvider::new("", None);
        assert!(!provider.has_api_key());
    }
}
