//! LLM HTTP Client
//!
//! This module provides a reusable HTTP client for making requests to LLM APIs,
//! with built-in retry logic, exponential backoff, and error handling.

use crate::error::{Result, SchemaForgeError};
use reqwest::header::{HeaderMap, HeaderValue, HeaderName, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use std::time::Duration;
use std::collections::HashMap;
use std::str::FromStr;

/// Default maximum number of retry attempts
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default initial retry delay in milliseconds
const DEFAULT_INITIAL_DELAY_MS: u64 = 1000;

/// Default timeout for HTTP requests (in seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// HTTP client for LLM API requests
#[derive(Clone)]
pub struct LLMHttpClient {
    /// Reqwest HTTP client
    client: Client,
    /// Maximum number of retry attempts
    max_retries: u32,
    /// Initial retry delay in milliseconds
    initial_delay_ms: u64,
}

impl LLMHttpClient {
    /// Create a new HTTP client with default settings
    pub fn new() -> Result<Self> {
        Self::with_timeout(DEFAULT_TIMEOUT_SECS)
    }

    /// Create a new HTTP client with custom timeout
    pub fn with_timeout(timeout_secs: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| SchemaForgeError::Http(e))?;

        Ok(Self {
            client,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_delay_ms: DEFAULT_INITIAL_DELAY_MS,
        })
    }

    /// Set the maximum number of retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set the initial retry delay
    pub fn with_retry_delay(mut self, delay_ms: u64) -> Self {
        self.initial_delay_ms = delay_ms;
        self
    }

    /// Make a POST request with retry logic
    ///
    /// # Arguments
    /// * `url` - Request URL
    /// * `headers` - Request headers
    /// * `body` - Request body (serializable)
    ///
    /// # Returns
    /// Response body as string
    pub async fn post_with_retry<T: Serialize>(
        &self,
        url: &str,
        headers: HeaderMap,
        body: &T,
    ) -> Result<String> {
        self.post_with_retry_internal(url, headers, body, 0).await
    }

    /// Internal POST implementation with retry logic
    async fn post_with_retry_internal<T: Serialize>(
        &self,
        url: &str,
        headers: HeaderMap,
        body: &T,
        attempt: u32,
    ) -> Result<String> {
        let response = self
            .client
            .post(url)
            .headers(headers.clone())
            .json(body)
            .send()
            .await
            .map_err(|e| SchemaForgeError::Http(e))?;

        let status = response.status();

        if status.is_success() {
            let text = response
                .text()
                .await
                .map_err(|e| SchemaForgeError::Http(e))?;
            return Ok(text);
        }

        // Check if we should retry
        if self.should_retry(status, attempt) {
            let delay = self.calculate_delay(attempt);
            tokio::time::sleep(Duration::from_millis(delay)).await;

            return Box::pin(self
                .post_with_retry_internal(url, headers, body, attempt + 1))
                .await;
        }

        // If we get here, the request failed and we shouldn't retry
        let response_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response body".to_string());

        Err(SchemaForgeError::LLMApiError {
            provider: "HTTP".to_string(),
            message: response_text.clone(),
            status: status.as_u16(),
        })
    }

    /// Check if a request should be retried
    fn should_retry(&self, status: StatusCode, attempt: u32) -> bool {
        if attempt >= self.max_retries {
            return false;
        }

        // Retry on rate limiting (429)
        if status == StatusCode::TOO_MANY_REQUESTS {
            return true;
        }

        // Retry on server errors (5xx)
        if status.is_server_error() {
            return true;
        }

        // Retry on connection issues (timeouts, etc.)
        if status == StatusCode::REQUEST_TIMEOUT || status == StatusCode::SERVICE_UNAVAILABLE {
            return true;
        }

        false
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_delay(&self, attempt: u32) -> u64 {
        // Exponential backoff: delay * 2^attempt
        self.initial_delay_ms * 2_u64.pow(attempt)
    }

    /// Build standard headers for API requests
    pub fn build_headers(api_key: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .expect("Invalid API key format"),
        );
        headers
    }

    /// Build headers with custom authorization format
    pub fn build_headers_with_auth(auth_header: &str, auth_value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let header_name = HeaderName::from_bytes(auth_header.as_bytes())
            .expect("Invalid header name format");
        headers.insert(
            header_name,
            HeaderValue::from_str(auth_value)
                .expect("Invalid auth value format"),
        );
        headers
    }

    /// Add custom header to existing headers
    pub fn add_header(mut headers: HeaderMap, key: &str, value: &str) -> Result<HeaderMap> {
        let key_header = HeaderName::from_str(key).map_err(|_| {
            SchemaForgeError::InvalidHeader(format!("Invalid header name: {}", key))
        })?;
        let value_header = HeaderValue::from_str(value).map_err(|_| {
            SchemaForgeError::InvalidHeader(format!("Invalid header value: {}", value))
        })?;

        headers.insert(key_header, value_header);
        Ok(headers)
    }
}

impl Default for LLMHttpClient {
    fn default() -> Self {
        Self::new().expect("Failed to create HTTP client")
    }
}

/// Helper for building API request bodies
pub struct RequestBody {
    /// Request body data
    pub data: HashMap<String, serde_json::Value>,
}

impl RequestBody {
    /// Create a new request body builder
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Add a field to the request body
    pub fn add_field(mut self, key: impl Into<String>, value: impl Serialize) -> Result<Self> {
        let json_value = serde_json::to_value(value)
            .map_err(|e| SchemaForgeError::Serialization(e))?;
        self.data.insert(key.into(), json_value);
        Ok(self)
    }

    /// Build the body as a HashMap
    pub fn build(self) -> Result<HashMap<String, serde_json::Value>> {
        Ok(self.data)
    }
}

impl Default for RequestBody {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_creation() {
        let client = LLMHttpClient::new().unwrap();
        assert_eq!(client.max_retries, DEFAULT_MAX_RETRIES);
    }

    #[test]
    fn test_http_client_with_timeout() {
        let client = LLMHttpClient::with_timeout(30).unwrap();
        assert_eq!(client.max_retries, DEFAULT_MAX_RETRIES);
    }

    #[test]
    fn test_retry_logic() {
        let client = LLMHttpClient::new().unwrap();

        // Should retry on server errors
        assert!(client.should_retry(StatusCode::INTERNAL_SERVER_ERROR, 0));
        assert!(client.should_retry(StatusCode::SERVICE_UNAVAILABLE, 0));

        // Should retry on rate limiting
        assert!(client.should_retry(StatusCode::TOO_MANY_REQUESTS, 0));

        // Should not retry on client errors
        assert!(!client.should_retry(StatusCode::BAD_REQUEST, 0));

        // Should not retry after max attempts
        assert!(!client.should_retry(StatusCode::INTERNAL_SERVER_ERROR, 5));
    }

    #[test]
    fn test_exponential_backoff() {
        let client = LLMHttpClient::new().unwrap();

        // First retry: 1000ms
        assert_eq!(client.calculate_delay(0), 1000);
        // Second retry: 2000ms
        assert_eq!(client.calculate_delay(1), 2000);
        // Third retry: 4000ms
        assert_eq!(client.calculate_delay(2), 4000);
    }

    #[test]
    fn test_headers_building() {
        let headers = LLMHttpClient::build_headers("test-key");
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
        assert_eq!(headers.get("authorization").unwrap(), "Bearer test-key");
    }

    #[test]
    fn test_request_body_builder() {
        let body = RequestBody::new()
            .add_field("model", "gpt-4")
            .unwrap()
            .add_field("max_tokens", 100)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(body.get("model").unwrap(), &serde_json::json!("gpt-4"));
        assert_eq!(body.get("max_tokens").unwrap(), &serde_json::json!(100));
    }
}
