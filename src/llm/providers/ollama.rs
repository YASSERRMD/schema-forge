//! Ollama local API provider.
//!
//! Uses Ollama's OpenAI-compatible chat completions endpoint so the app can
//! treat local Ollama models like the other chat providers.

use crate::error::{Result, SchemaForgeError};
use crate::llm::client::LLMHttpClient;
use crate::llm::provider::{GenerationParams, LLMResponse, LLMProvider, Message, MessageRole};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434";

pub struct OllamaProvider {
    api_key: String,
    model: String,
    client: LLMHttpClient,
    endpoint: String,
    max_tokens: u32,
}

impl OllamaProvider {
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        let model = model.unwrap_or_else(|| "llama3.2".to_string());
        Self {
            api_key: api_key.into(),
            model,
            client: LLMHttpClient::new().expect("Failed to create HTTP client"),
            endpoint: ollama_chat_completions_url(
                &std::env::var("OLLAMA_BASE_URL")
                    .unwrap_or_else(|_| DEFAULT_OLLAMA_BASE_URL.to_string()),
            ),
            max_tokens: 4096,
        }
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    fn build_headers(&self) -> reqwest::header::HeaderMap {
        // Ollama ignores the bearer token for local usage, but keeping the
        // OpenAI-compatible auth shape avoids special-casing the HTTP client.
        LLMHttpClient::build_headers(&self.api_key)
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| OllamaMessage {
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

    fn extract_content(&self, response: &OllamaResponse) -> String {
        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    async fn generate(
        &self,
        messages: &[Message],
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let request = OllamaRequest {
            model: self.model.clone(),
            messages: self.convert_messages(messages),
            max_tokens: Some(params.and_then(|p| p.max_tokens).unwrap_or(self.max_tokens)),
            temperature: Some(params.and_then(|p| p.temperature).unwrap_or(0.2)),
            top_p: params.and_then(|p| p.top_p),
            stop: params.and_then(|p| p.stop_sequences.clone()),
            stream: Some(false),
        };

        let response_text = self
            .client
            .post_with_retry(&self.endpoint, self.build_headers(), &request)
            .await?;

        let response: OllamaResponse = serde_json::from_str(&response_text).map_err(|e| {
            SchemaForgeError::LLMApiError {
                provider: "Ollama".to_string(),
                message: format!("Failed to parse response: {}", e),
                status: 0,
            }
        })?;

        Ok(LLMResponse {
            content: self.extract_content(&response),
            model: Some(response.model),
            input_tokens: response.usage.as_ref().map(|u| u.prompt_tokens),
            output_tokens: response.usage.as_ref().map(|u| u.completion_tokens),
            total_tokens: response.usage.as_ref().map(|u| u.total_tokens),
            finish_reason: response.choices.first().and_then(|c| c.finish_reason.clone()),
        })
    }

    async fn generate_with_schema(
        &self,
        schema_context: &str,
        user_query: &str,
        params: Option<&GenerationParams>,
    ) -> Result<LLMResponse> {
        let messages = vec![
            Message::system(format!(
                "You are a database expert. Answer questions about database schemas based on the provided context.\n\nDatabase Schema:\n{}",
                schema_context
            )),
            Message::user(user_query),
        ];

        self.generate(&messages, params).await
    }

    async fn generate_sql(
        &self,
        schema_context: &str,
        natural_language_query: &str,
    ) -> Result<String> {
        let messages = vec![
            Message::system(
                "You are a SQL expert. Convert natural language queries to SQL based on the provided database schema.\n\nRules:\n1. Return ONLY the SQL query, no explanations\n2. Use proper table and column names from the schema\n3. Handle NULL values appropriately\n4. Use proper JOIN syntax\n5. Add appropriate WHERE clauses\n6. Format SQL in a readable way\n7. Return plain SQL without markdown fences.",
            ),
            Message::user(format!(
                "Database Schema:\n{}\n\nQuery: {}",
                schema_context, natural_language_query
            )),
        ];

        Ok(self.generate(&messages, None).await?.content.trim().to_string())
    }

    fn provider_name(&self) -> &str {
        "Ollama"
    }

    fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }
}

fn ollama_chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');

    if trimmed.ends_with("/v1/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    model: String,
    choices: Vec<OllamaChoice>,
    usage: Option<OllamaUsage>,
}

#[derive(Debug, Deserialize, Clone)]
struct OllamaChoice {
    message: OllamaMessageResponse,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct OllamaMessageResponse {
    content: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct OllamaUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaProvider::new("ollama", None);
        assert_eq!(provider.model, "llama3.2");
        assert!(provider.has_api_key());
    }

    #[test]
    fn test_ollama_provider_with_custom_model() {
        let provider = OllamaProvider::new("ollama", Some("qwen2.5-coder".to_string()));
        assert_eq!(provider.model, "qwen2.5-coder");
    }

    #[test]
    fn test_ollama_provider_with_max_tokens() {
        let provider = OllamaProvider::new("ollama", None).with_max_tokens(2048);
        assert_eq!(provider.max_tokens, 2048);
    }

    #[test]
    fn test_ollama_chat_completions_url_normalization() {
        assert_eq!(
            ollama_chat_completions_url("http://localhost:11434"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            ollama_chat_completions_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            ollama_chat_completions_url("http://localhost:11434/v1/chat/completions"),
            "http://localhost:11434/v1/chat/completions"
        );
    }
}
