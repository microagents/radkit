//! Common test utilities and setup

use radkit::config::EnvKey;
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
/// Returns an EnvKey if the environment variable is available, None otherwise.
pub fn get_api_key(key_name: &str) -> Option<EnvKey> {
    init_test_env();
    // Check if the environment variable exists
    std::env::var(key_name).ok().map(|_| EnvKey::new(key_name))
}

/// Get Anthropic API key EnvKey if available
pub fn get_anthropic_key() -> Option<EnvKey> {
    get_api_key("ANTHROPIC_API_KEY")
}

/// Get Gemini API key EnvKey if available
pub fn get_gemini_key() -> Option<EnvKey> {
    get_api_key("GEMINI_API_KEY")
}

/// Get OpenAI API key EnvKey if available
pub fn get_openai_key() -> Option<EnvKey> {
    get_api_key("OPENAI_API_KEY")
}
