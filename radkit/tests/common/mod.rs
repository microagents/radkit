//! Common test utilities and setup

use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment by loading .env file if it exists.
/// This should be called at the beginning of each test that needs environment variables.
pub fn init_test_env() {
    INIT.call_once(|| {
        // Try to load .env file, but don't fail if it doesn't exist
        if let Err(_) = dotenvy::dotenv() {
            // .env file not found or couldn't be read, which is fine
            // Tests will skip gracefully if API keys are not available
        }
    });
}

/// Helper function to check if we have the required API key for testing.
/// Returns the API key if available, None otherwise.
pub fn get_api_key(key_name: &str) -> Option<String> {
    init_test_env();
    std::env::var(key_name).ok()
}

/// Get Anthropic API key if available
pub fn get_anthropic_key() -> Option<String> {
    get_api_key("ANTHROPIC_API_KEY")
}

/// Get Gemini API key if available
pub fn get_gemini_key() -> Option<String> {
    get_api_key("GEMINI_API_KEY")
}

/// Get OpenAI API key if available
pub fn get_openai_key() -> Option<String> {
    get_api_key("OPENAI_API_KEY")
}
