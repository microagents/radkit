use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<JsonRpcId>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Option<JsonRpcId>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC ID can be string, number, or null
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(i64),
    String(String),
    Null,
}

impl JsonRpcResponse {
    pub fn success(id: Option<JsonRpcId>, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: Option<JsonRpcId>, code: i32, message: String, data: Option<Value>) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data,
            }),
            id,
        }
    }
}

/// Helper function to validate JSON-RPC request
pub fn validate_request(req: &JsonRpcRequest) -> Result<(), crate::Error> {
    if req.jsonrpc != "2.0" {
        return Err(crate::Error::InvalidRequest(
            "Invalid JSON-RPC version".to_string(),
        ));
    }
    Ok(())
}
