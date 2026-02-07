//! LLM integration module
//!
//! This module provides trait-based LLM provider abstraction
//! and implementations for various AI services.

pub mod client;
pub mod provider;

// Provider implementations
pub mod providers {
    pub mod anthropic;
    pub mod cohere;
    pub mod groq;
    pub mod minimax;
    pub mod openai;
    pub mod qwen;
    pub mod xai;
    pub mod zai;

    // Re-export provider implementations
    pub use anthropic::AnthropicProvider;
    pub use cohere::CohereProvider;
    pub use groq::GroqProvider;
    pub use minimax::MinimaxProvider;
    pub use openai::OpenAIProvider;
    pub use qwen::QwenProvider;
    pub use xai::XAIProvider;
    pub use zai::ZAIProvider;
}

// Re-exports
pub use client::{LLMHttpClient, RequestBody};
pub use provider::{
    GenerationParams, LLMProvider, LLMProviderBuilder, LLMResponse, Message, MessageRole,
};
