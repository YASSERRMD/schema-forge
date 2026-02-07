//! OpenAI API Provider
//!
//! This module implements the LLMProvider trait for OpenAI's GPT API.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

/// OpenAI API base URL
const OPENAI_API_BASE: &str = "https://api.openai.com/v1/chat/completions";

/// OpenAI GPT API provider
pub struct OpenAIProvider {
    /// API key for authentication
    api_key: String,
    /// Model to use (e.g., "gpt-4", "gpt-3.5-turbo")
    model: String,
    /// HTTP client for making requests
    client: LLMHttpClient,
    /// Maximum tokens for generation
    max_tokens: u32,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider
    ///
    /// # Arguments
    /// * `api_key` - OpenAI API key
    /// * `model` - Model identifier (defaults to gpt-4o-mini)
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "gpt-4o-mini".to_string());
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

    /// Build headers for OpenAI API
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        LLMHttpClient::build_headers(&self.api_key)
    }

    /// Convert our Message format to OpenAI format
    fn convert_messages_to_openai(&self, messages: &[Message]) -> Vec<OpenAIMessage> {
        messages
            .iter()
            .map(|msg| OpenAIMessage {
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

    /// Extract text content from OpenAI response
    fn extract_content(&self, response: &OpenAIResponse) -> String {
        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    /// Generate a response from the GPT API
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let max_tokens = params
            .and_then(|p| p.max_tokens)
            .unwrap_or(self.max_tokens);

        let temperature: f32 = params.and_then(|p| p.temperature).unwrap_or(0.7);

        let openai_messages = self.convert_messages_to_openai(messages);

        let request = OpenAIRequest {
            model: self.model.clone(),
            messages: openai_messages,
            max_tokens: Some(max_tokens),
            temperature: Some(temperature),
            top_p: params.and_then(|p| p.top_p),
            stop: params.and_then(|p| p.stop_sequences.clone()),
        };

        let headers = self.build_headers();
        let response_text = self
            .client
            .post_with_retry(OPENAI_API_BASE, headers, &request)
            .await?;

        let openai_response: OpenAIResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                SchemaForgeError::LLMApiError {
                    provider: "OpenAI".to_string(),
                    message: format!("Failed to parse response: {}", e),
                    status: 0,
                }
            })?;

        let content = self.extract_content(&openai_response);

        Ok(LLMResponse {
            content,
            model: Some(openai_response.model),
            input_tokens: openai_response.usage.as_ref().map(|u| u.prompt_tokens),
            output_tokens: openai_response.usage.as_ref().map(|u| u.completion_tokens),
            total_tokens: openai_response.usage.as_ref().map(|u| u.total_tokens),
            finish_reason: openai_response.choices.first().and_then(|c| c.finish_reason.clone()),
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
7. For PostgreSQL, use ::text for type casting
8. For MySQL, use CAST for type casting
9. For SQLite, use CAST for type casting
10. For MSSQL, use CAST for type casting

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
        "OpenAI"
    }

    /// Check if API key is set
    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

/// OpenAI API request format
#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

/// OpenAI API message format
#[derive(Debug, Serialize, Clone)]
struct OpenAIMessage {
    role: String,
    content: String,
}

/// OpenAI API response format
#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

/// Choice in OpenAI response
#[derive(Debug, Deserialize, Clone)]
struct Choice {
    index: u32,
    message: OpenAIMessageResponse,
    finish_reason: Option<String>,
}

/// Message in OpenAI response
#[derive(Debug, Deserialize, Clone)]
struct OpenAIMessageResponse {
    role: String,
    content: Option<String>,
}

/// Token usage information
#[derive(Debug, Deserialize, Clone)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAIProvider::new("test-key", None);
        assert_eq!(provider.model, "gpt-4o-mini");
        assert_eq!(provider.max_tokens, 4096);
    }

    #[test]
    fn test_openai_provider_with_custom_model() {
        let provider = OpenAIProvider::new("test-key", Some("gpt-4".to_string()));
        assert_eq!(provider.model, "gpt-4");
    }

    #[test]
    fn test_openai_provider_with_max_tokens() {
        let provider = OpenAIProvider::new("test-key", None).with_max_tokens(8192);
        assert_eq!(provider.max_tokens, 8192);
    }

    #[test]
    fn test_message_conversion() {
        let provider = OpenAIProvider::new("test-key", None);

        let messages = vec![
            Message {
                role: MessageRole::System,
                content: "You are a helpful assistant.".to_string(),
            },
            Message {
                role: MessageRole::User,
                content: "Hello".to_string(),
            },
            Message {
                role: MessageRole::Assistant,
                content: "Hi there!".to_string(),
            },
        ];

        let openai_messages = provider.convert_messages_to_openai(&messages);
        assert_eq!(openai_messages.len(), 3);
        assert_eq!(openai_messages[0].role, "system");
        assert_eq!(openai_messages[0].content, "You are a helpful assistant.");
        assert_eq!(openai_messages[1].role, "user");
        assert_eq!(openai_messages[1].content, "Hello");
        assert_eq!(openai_messages[2].role, "assistant");
        assert_eq!(openai_messages[2].content, "Hi there!");
    }

    #[test]
    fn test_has_api_key() {
        let provider = OpenAIProvider::new("test-key", None);
        assert!(provider.has_api_key());

        let provider = OpenAIProvider::new("", None);
        assert!(!provider.has_api_key());
    }
}
