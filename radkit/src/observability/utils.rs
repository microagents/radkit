//! Utility functions for observability
//!
//! This module provides utilities for PII redaction, cost calculation,
//! and span recording helpers.

use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::RwLock;
use tracing::Span;

use crate::observability::config::TelemetryConfig;

/// Global configuration storage for Pattern 4 (use-global mode)
/// Single source of truth - no duplicate storage
static GLOBAL_CONFIG: Lazy<RwLock<Option<TelemetryConfig>>> = Lazy::new(|| RwLock::new(None));

/// Hash user ID for GDPR compliance
///
/// Uses SHA256 to create a deterministic, anonymized identifier.
/// Returns first 16 characters of hex-encoded hash.
pub fn hash_user_id(user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(user_id.as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_string()
}

/// Sanitize tool arguments to remove sensitive information
///
/// Redacts API keys, tokens, passwords, and authorization headers.
/// Also truncates to max_length to prevent excessive trace sizes.
pub fn sanitize_tool_args(args: &str, max_length: usize) -> String {
    static SENSITIVE_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
        vec![
            (
                Regex::new(r#"["']?(api[_-]?key|token|password)["']?\s*[:=]\s*['"]?[^'"]+['"]?"#)
                    .expect("Failed to compile regex"),
                "$1: [REDACTED]",
            ),
            (
                Regex::new(r"(?i)(authorization|bearer)\s*:\s*.+")
                    .expect("Failed to compile regex"),
                "$1: [REDACTED]",
            ),
        ]
    });

    let mut sanitized = args.to_string();
    for (re, replacement) in SENSITIVE_PATTERNS.iter() {
        sanitized = re.replace_all(&sanitized, *replacement).to_string();
    }

    if sanitized.len() > max_length {
        sanitized = format!("{}...[truncated]", &sanitized[..max_length]);
    }

    sanitized
}

/// Record LLM token usage on current span
pub fn record_llm_tokens(prompt_tokens: u64, completion_tokens: u64, total_tokens: u64) {
    let span = Span::current();
    span.record("llm.response.prompt_tokens", prompt_tokens);
    span.record("llm.response.completion_tokens", completion_tokens);
    span.record("llm.response.total_tokens", total_tokens);
}

/// Calculate LLM cost with configurable pricing
///
/// First checks custom pricing from global config, then falls back to built-in defaults.
/// Returns cost in USD.
pub fn calculate_llm_cost(model: &str, prompt_tokens: u64, completion_tokens: u64) -> f64 {
    // Check custom pricing from global config (single source of truth)
    if let Ok(config) = GLOBAL_CONFIG.read() {
        if let Some(cfg) = config.as_ref() {
            if let Some(custom) = cfg.llm_pricing.iter().find(|p| p.model == model) {
                return (prompt_tokens as f64 * custom.prompt_price / 1000.0)
                    + (completion_tokens as f64 * custom.completion_price / 1000.0);
            }
        }
    }

    // Fall back to built-in defaults (prices per 1K tokens in USD)
    let (prompt_price, completion_price) = match model {
        // OpenAI GPT-4
        "gpt-4" | "gpt-4-0613" => (0.03, 0.06),
        "gpt-4-32k" => (0.06, 0.12),
        "gpt-4-turbo" | "gpt-4-turbo-preview" => (0.01, 0.03),
        "gpt-4o" | "gpt-4o-2024-05-13" => (0.005, 0.015),
        "gpt-4o-mini" => (0.00015, 0.0006),

        // OpenAI GPT-3.5
        "gpt-3.5-turbo" | "gpt-3.5-turbo-0125" => (0.0005, 0.0015),

        // Anthropic Claude
        "claude-3-opus-20240229" => (0.015, 0.075),
        "claude-3-sonnet-20240229" => (0.003, 0.015),
        "claude-3-haiku-20240307" => (0.00025, 0.00125),
        "claude-3-5-sonnet-20240620" => (0.003, 0.015),

        // Google Gemini
        "gemini-1.5-pro" => (0.00125, 0.005),
        "gemini-1.5-flash" => (0.000075, 0.0003),

        _ => {
            tracing::warn!("Unknown LLM model '{}' - cost will be $0.00", model);
            (0.0, 0.0)
        }
    };

    (prompt_tokens as f64 * prompt_price / 1000.0)
        + (completion_tokens as f64 * completion_price / 1000.0)
}

/// Record LLM cost on current span
pub fn record_llm_cost(model: &str, prompt_tokens: u64, completion_tokens: u64) {
    let cost = calculate_llm_cost(model, prompt_tokens, completion_tokens);
    Span::current().record("llm.cost_usd", cost);
}

/// Record error on current span
pub fn record_error(error: &dyn std::error::Error) {
    let span = Span::current();
    span.record("otel.status_code", "ERROR");
    span.record("error.message", error.to_string().as_str());
}

/// Record success on current span
pub fn record_success() {
    let span = Span::current();
    span.record("otel.status_code", "OK");
}

/// Record agent message metric
pub fn record_agent_message_metric(agent_name: &str, success: bool) {
    crate::observability::metrics::record_agent_message_metric(agent_name, success);
}

/// Record LLM token usage metric
pub fn record_llm_tokens_metric(model: &str, prompt_tokens: u64, completion_tokens: u64) {
    crate::observability::metrics::record_llm_tokens_metric(model, prompt_tokens, completion_tokens);
}

/// Record tool execution metric
pub fn record_tool_execution_metric(tool_name: &str, success: bool) {
    crate::observability::metrics::record_tool_execution_metric(tool_name, success);
}

/// Store global config for Pattern 4 (use-global mode)
///
/// This is called by `configure_radkit_telemetry()` to store configuration
/// that can be accessed by utility functions.
pub(crate) fn set_global_config(config: TelemetryConfig) {
    if let Ok(mut cfg) = GLOBAL_CONFIG.write() {
        *cfg = Some(config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_user_id_deterministic() {
        let hash1 = hash_user_id("user@example.com");
        let hash2 = hash_user_id("user@example.com");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_hash_user_id_different_inputs() {
        let hash1 = hash_user_id("user1@example.com");
        let hash2 = hash_user_id("user2@example.com");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_sanitize_tool_args_api_key() {
        let input = r#"{"api_key": "sk-1234567890abcdef"}"#;
        let sanitized = sanitize_tool_args(input, 1000);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("sk-1234567890abcdef"));
    }

    #[test]
    fn test_sanitize_tool_args_bearer_token() {
        let input = "Authorization: Bearer abc123xyz";
        let sanitized = sanitize_tool_args(input, 1000);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("abc123xyz"));
    }

    #[test]
    fn test_sanitize_tool_args_truncation() {
        let input = "a".repeat(200);
        let sanitized = sanitize_tool_args(&input, 100);
        assert!(sanitized.contains("[truncated]"));
        assert!(sanitized.len() <= 115); // 100 + "[truncated]"
    }

    #[test]
    fn test_calculate_llm_cost_gpt4() {
        let cost = calculate_llm_cost("gpt-4", 1000, 500);
        // $0.03/1K prompt + $0.06/1K completion
        let expected = (1000.0 * 0.03 / 1000.0) + (500.0 * 0.06 / 1000.0);
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn test_calculate_llm_cost_claude() {
        let cost = calculate_llm_cost("claude-3-opus-20240229", 2000, 1000);
        // $0.015/1K prompt + $0.075/1K completion
        let expected = (2000.0 * 0.015 / 1000.0) + (1000.0 * 0.075 / 1000.0);
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn test_calculate_llm_cost_unknown_model() {
        let cost = calculate_llm_cost("unknown-model", 1000, 500);
        assert_eq!(cost, 0.0);
    }
}
