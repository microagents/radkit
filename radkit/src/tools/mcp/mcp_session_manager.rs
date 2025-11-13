use crate::errors::{AgentError, AgentResult};
use dashmap::DashMap;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rmcp::{
    model::{
        ClientCapabilities, ClientInfo, Implementation, InitializeRequestParam, ProtocolVersion,
    },
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, ConfigureCommandExt,
        StreamableHttpClientTransport, TokioChildProcess,
    },
    ServiceExt,
};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Type alias for the MCP client - based on rmcp examples
pub type MCPClient =
    rmcp::service::RunningService<rmcp::service::RoleClient, InitializeRequestParam>;

/// Connection parameters for MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPConnectionParams {
    /// Connect to a local MCP server via stdio
    Stdio {
        command: String,
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default = "default_timeout")]
        timeout: Duration,
    },
    /// Connect to an MCP server via HTTP with SSE support
    Http {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default = "default_timeout")]
        timeout: Duration,
    },
}

const fn default_timeout() -> Duration {
    Duration::from_secs(5)
}

/// Manages MCP client sessions with pooling and automatic reconnection
pub struct MCPSessionManager {
    connection_params: MCPConnectionParams,
    /// Session pool: maps session keys to client instances
    sessions: Arc<DashMap<String, Arc<MCPClient>>>,
}

impl MCPSessionManager {
    /// Create a new session manager with the given connection parameters
    #[must_use]
    pub fn new(params: MCPConnectionParams) -> Self {
        Self {
            connection_params: params,
            sessions: Arc::new(DashMap::new()),
        }
    }

    /// Generate a session key based on connection parameters and headers
    /// This ensures each unique connection configuration gets its own session
    fn generate_session_key(&self, headers: Option<&HashMap<String, String>>) -> String {
        let mut hasher = DefaultHasher::new();

        match &self.connection_params {
            MCPConnectionParams::Stdio {
                command,
                args,
                env,
                timeout,
            } => {
                // Hash the stdio connection parameters to create unique session keys
                "stdio".hash(&mut hasher);
                command.hash(&mut hasher);
                for arg in args {
                    arg.hash(&mut hasher);
                }
                // Hash environment variables in a deterministic order
                let mut env_sorted: Vec<_> = env.iter().collect();
                env_sorted.sort_by_key(|&(k, _)| k);
                for (k, v) in env_sorted {
                    k.hash(&mut hasher);
                    v.hash(&mut hasher);
                }
                timeout.as_millis().hash(&mut hasher);
                format!("stdio_{:x}", hasher.finish())
            }
            MCPConnectionParams::Http {
                url,
                headers: param_headers,
                timeout,
            } => {
                // Hash the HTTP URL, headers from params, and timeout
                "http".hash(&mut hasher);
                url.hash(&mut hasher);

                // Hash headers from connection params (base headers for the client)
                let mut sorted: Vec<_> = param_headers.iter().collect();
                sorted.sort_by_key(|&(k, _)| k);
                for (k, v) in sorted {
                    k.hash(&mut hasher);
                    v.hash(&mut hasher);
                }

                // Also hash per-session headers if provided
                if let Some(hdrs) = headers {
                    let mut sorted: Vec<_> = hdrs.iter().collect();
                    sorted.sort_by_key(|&(k, _)| k);
                    for (k, v) in sorted {
                        k.hash(&mut hasher);
                        v.hash(&mut hasher);
                    }
                }

                // Hash timeout like we do for stdio
                timeout.as_millis().hash(&mut hasher);

                format!("http_{:x}", hasher.finish())
            }
        }
    }

    /// Check if a client is disconnected by attempting a lightweight operation
    async fn is_disconnected(&self, client: &MCPClient) -> bool {
        // Try to list tools as a lightweight way to check if connection is alive
        // This is an async operation that will fail if the connection is dead
        if let Ok(Ok(_)) = tokio::time::timeout(
            Duration::from_millis(500), // Short timeout for quick check
            client.list_all_tools(),
        )
        .await
        {
            // Connection is alive and tools were listed successfully
            false
        } else {
            // Either an MCP error or timeout indicates disconnection
            debug!("Connection check failed - assuming disconnected");
            true
        }
    }

    /// Create a new MCP client with the configured parameters
    async fn create_client(&self) -> AgentResult<MCPClient> {
        match &self.connection_params {
            MCPConnectionParams::Stdio {
                command, args, env, ..
            } => {
                debug!("Creating stdio MCP client: {} {:?}", command, args);

                let mut cmd = Command::new(command);
                cmd.args(args);
                for (key, value) in env {
                    cmd.env(key, value);
                }

                let transport = TokioChildProcess::new(cmd.configure(|_| {})).map_err(|e| {
                    AgentError::ToolSetupFailed {
                        tool_name: "mcp_stdio".to_string(),
                        reason: format!("Failed to spawn MCP process: {e}"),
                    }
                })?;

                // Create proper client info for MCP handshake
                let client_info = ClientInfo {
                    protocol_version: ProtocolVersion::default(),
                    capabilities: ClientCapabilities::default(),
                    client_info: Implementation {
                        name: "radkit-mcp-client".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        title: None,
                        website_url: None,
                        icons: None,
                    },
                };

                let client = client_info.serve(transport).await.map_err(|e| {
                    AgentError::ToolSetupFailed {
                        tool_name: "mcp_stdio".to_string(),
                        reason: format!("Failed to connect to MCP server: {e:?}"),
                    }
                })?;

                info!("Connected to MCP server via stdio");
                Ok(client)
            }
            MCPConnectionParams::Http {
                url,
                headers,
                timeout,
            } => {
                debug!("Creating HTTP MCP client for: {}", url);

                // Build custom reqwest client with headers and timeout
                let mut header_map = HeaderMap::new();
                for (key, value) in headers {
                    let header_name = HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
                        AgentError::ToolSetupFailed {
                            tool_name: "mcp_http".to_string(),
                            reason: format!("Invalid header name '{key}': {e}"),
                        }
                    })?;
                    let header_value =
                        HeaderValue::from_str(value).map_err(|e| AgentError::ToolSetupFailed {
                            tool_name: "mcp_http".to_string(),
                            reason: format!("Invalid header value for '{key}': {e}"),
                        })?;
                    header_map.insert(header_name, header_value);
                }

                let reqwest_client = reqwest::Client::builder()
                    .default_headers(header_map)
                    .timeout(*timeout)
                    .build()
                    .map_err(|e| AgentError::ToolSetupFailed {
                        tool_name: "mcp_http".to_string(),
                        reason: format!("Failed to build HTTP client: {e}"),
                    })?;

                // Create HTTP transport with custom client and config
                let config = StreamableHttpClientTransportConfig::with_uri(url.clone());
                let transport = StreamableHttpClientTransport::with_client(reqwest_client, config);

                // Create proper client info for MCP handshake
                let client_info = ClientInfo {
                    protocol_version: ProtocolVersion::default(),
                    capabilities: ClientCapabilities::default(),
                    client_info: Implementation {
                        name: "radkit-mcp-client".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        title: None,
                        website_url: None,
                        icons: None,
                    },
                };

                let client = client_info.serve(transport).await.map_err(|e| {
                    AgentError::ToolSetupFailed {
                        tool_name: "mcp_http".to_string(),
                        reason: format!("Failed to connect to MCP HTTP server: {e:?}"),
                    }
                })?;

                info!("Connected to MCP server via HTTP: {}", url);
                Ok(client)
            }
        }
    }

    /// Creates or retrieves a session, handling disconnections automatically
    ///
    /// # Errors
    ///
    /// Returns an error if the MCP server connection cannot be established
    pub async fn create_session(
        &self,
        headers: Option<HashMap<String, String>>,
    ) -> AgentResult<Arc<MCPClient>> {
        let session_key = self.generate_session_key(headers.as_ref());

        // Check if we have an existing session
        if let Some(existing) = self.sessions.get(&session_key) {
            let client = existing.value().clone();
            if !self.is_disconnected(&client).await {
                debug!("Reusing existing MCP session: {}", session_key);
                return Ok(client);
            }
            // Session is disconnected, remove it
            drop(existing); // Release the read lock
            self.sessions.remove(&session_key);
            info!("Removed disconnected MCP session: {}", session_key);
        }

        // Create new session
        info!("Creating new MCP session: {}", session_key);
        let client = self.create_client().await?;
        let client_arc = Arc::new(client);

        // Store in session pool
        self.sessions
            .insert(session_key.clone(), client_arc.clone());

        Ok(client_arc)
    }

    /// Close all sessions and clean up resources
    pub async fn close(&self) {
        info!("Closing all MCP sessions");
        let session_keys: Vec<_> = self
            .sessions
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for key in session_keys {
            if let Some((_, client)) = self.sessions.remove(&key) {
                // Try to get ownership of the client for cancellation
                match Arc::try_unwrap(client) {
                    Ok(owned_client) => {
                        if let Err(e) = owned_client.cancel().await {
                            warn!("Error closing MCP session {}: {:?}", key, e);
                        }
                    }
                    Err(shared_client) => {
                        warn!(
                            "Could not close MCP session {}: still has {} references",
                            key,
                            Arc::strong_count(&shared_client)
                        );
                    }
                }
            }
        }

        self.sessions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_params_with_headers_and_timeout() {
        // Verify HTTP params can be created with headers and timeout
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());
        headers.insert("X-Custom-Header".to_string(), "custom-value".to_string());

        let params = MCPConnectionParams::Http {
            url: "https://example.com/mcp".to_string(),
            headers: headers.clone(),
            timeout: Duration::from_secs(30),
        };

        // Verify params are properly structured
        if let MCPConnectionParams::Http {
            url,
            headers: h,
            timeout,
        } = params
        {
            assert_eq!(url, "https://example.com/mcp");
            assert_eq!(h.len(), 2);
            assert_eq!(h.get("Authorization"), Some(&"Bearer token123".to_string()));
            assert_eq!(h.get("X-Custom-Header"), Some(&"custom-value".to_string()));
            assert_eq!(timeout, Duration::from_secs(30));
        } else {
            panic!("Expected HTTP params");
        }
    }

    #[test]
    fn test_session_key_generation_includes_headers() {
        // Create manager with HTTP params
        let mut headers1 = HashMap::new();
        headers1.insert("Authorization".to_string(), "Bearer token1".to_string());

        let params1 = MCPConnectionParams::Http {
            url: "https://example.com/mcp".to_string(),
            headers: headers1.clone(),
            timeout: Duration::from_secs(30),
        };

        let manager1 = MCPSessionManager::new(params1);
        let key1 = manager1.generate_session_key(None);

        // Create manager with different headers
        let mut headers2 = HashMap::new();
        headers2.insert("Authorization".to_string(), "Bearer token2".to_string());

        let params2 = MCPConnectionParams::Http {
            url: "https://example.com/mcp".to_string(),
            headers: headers2.clone(),
            timeout: Duration::from_secs(30),
        };

        let manager2 = MCPSessionManager::new(params2);
        let key2 = manager2.generate_session_key(None);

        // Different headers should produce different session keys
        assert_ne!(key1, key2, "Session keys should differ when headers differ");
    }

    #[test]
    fn test_session_key_generation_http_includes_url() {
        // Same headers, different URLs
        let headers = HashMap::new();

        let params1 = MCPConnectionParams::Http {
            url: "https://example.com/mcp1".to_string(),
            headers: headers.clone(),
            timeout: Duration::from_secs(30),
        };

        let manager1 = MCPSessionManager::new(params1);
        let key1 = manager1.generate_session_key(None);

        let params2 = MCPConnectionParams::Http {
            url: "https://example.com/mcp2".to_string(),
            headers,
            timeout: Duration::from_secs(30),
        };

        let manager2 = MCPSessionManager::new(params2);
        let key2 = manager2.generate_session_key(None);

        // Different URLs should produce different session keys
        assert_ne!(key1, key2, "Session keys should differ when URLs differ");

        // Both should start with "http_"
        assert!(key1.starts_with("http_"));
        assert!(key2.starts_with("http_"));
    }

    #[test]
    fn test_session_key_stdio_vs_http() {
        // Create stdio params
        let stdio_params = MCPConnectionParams::Stdio {
            command: "test".to_string(),
            args: vec![],
            env: HashMap::new(),
            timeout: Duration::from_secs(5),
        };

        let stdio_manager = MCPSessionManager::new(stdio_params);
        let stdio_key = stdio_manager.generate_session_key(None);

        // Create HTTP params
        let http_params = MCPConnectionParams::Http {
            url: "https://example.com/mcp".to_string(),
            headers: HashMap::new(),
            timeout: Duration::from_secs(30),
        };

        let http_manager = MCPSessionManager::new(http_params);
        let http_key = http_manager.generate_session_key(None);

        // Different connection types should produce different keys
        assert_ne!(stdio_key, http_key);
        assert!(stdio_key.starts_with("stdio_"));
        assert!(http_key.starts_with("http_"));
    }

    #[test]
    fn test_session_key_deterministic() {
        // Same params should produce same key
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());

        let params1 = MCPConnectionParams::Http {
            url: "https://example.com/mcp".to_string(),
            headers: headers.clone(),
            timeout: Duration::from_secs(30),
        };

        let manager1 = MCPSessionManager::new(params1);

        let params2 = MCPConnectionParams::Http {
            url: "https://example.com/mcp".to_string(),
            headers: headers.clone(),
            timeout: Duration::from_secs(30),
        };

        let manager2 = MCPSessionManager::new(params2);

        let key1 = manager1.generate_session_key(None);
        let key2 = manager2.generate_session_key(None);

        // Same params should produce same session key
        assert_eq!(
            key1, key2,
            "Session keys should be deterministic for same params"
        );
    }
}
