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
    
    
    
    
    
    
    
    
}

// Re-exports
