pub mod anthropic;
pub mod openai;
pub mod provider;

pub use anthropic::AnthropicProvider;
pub use openai::OpenAIProvider;
pub use provider::{
    Backend, CompletionParams, CompletionResponse, ContentBlock, ContentType, FinishReason,
    LlmProvider, Message, ProviderConfig, Role, ToolCall, ToolCallResult, ToolDef,
};

use anyhow::{bail, Result};

/// Factory: build the right provider from config.
pub fn new_provider(cfg: ProviderConfig) -> Result<Box<dyn LlmProvider + Send + Sync>> {
    match cfg.backend {
        Backend::OpenAI => Ok(Box::new(OpenAIProvider::new(cfg))),
        Backend::Anthropic => Ok(Box::new(AnthropicProvider::new(cfg))),
    }
}

/// Auto-detect backend from model name.
pub fn detect_backend(model: &str) -> Backend {
    if model.starts_with("claude") {
        Backend::Anthropic
    } else {
        Backend::OpenAI
    }
}

/// Read provider config from environment variables.
pub fn config_from_env() -> Result<ProviderConfig> {
    dotenvy::dotenv().ok();

    let model = std::env::var("ARCHCODE_MODEL").unwrap_or_else(|_| "gpt-4o".into());
    let api_key = std::env::var("ARCHCODE_API_KEY").unwrap_or_default();
    let base_url = std::env::var("ARCHCODE_BASE_URL").unwrap_or_default();
    let backend = if let Ok(b) = std::env::var("ARCHCODE_PROVIDER") {
        match b.as_str() {
            "anthropic" => Backend::Anthropic,
            _ => Backend::OpenAI,
        }
    } else {
        detect_backend(&model)
    };

    Ok(ProviderConfig {
        model,
        api_key,
        base_url,
        backend,
    })
}
