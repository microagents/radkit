//! # A2A (Agent2Agent) Protocol Types
//!
//! This crate provides the Rust data structures for the Agent2Agent (A2A) protocol,
//! a standard for interoperability between AI agents. The types are derived from the
//! official A2A JSON Schema for that specific release
//! and are designed for serialization and deserialization with `serde`.
//!
//! The primary goal of A2A is to enable agents to:
//! - Discover each other's capabilities via the `AgentCard`.
//! - Negotiate interaction modalities (text, files, structured data).
//! - Manage collaborative `Task`s.
//! - Securely exchange information as `Message`s.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// JSON-RPC 2.0 Base Types (from schema)
// ============================================================================

// Re-export agent card types
pub mod agent_card;
pub use agent_card::{
    AgentCapabilities, AgentCard, AgentCardSignature, AgentExtension, AgentInterface,
    AgentProvider, AgentSkill, TransportProtocol,
};

/// Defines the base structure for any JSON-RPC 2.0 request, response, or notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCMessage {
    /// The version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: String,
    /// A unique identifier established by the client. It must be a String, a Number, or null.
    /// The server must reply with the same value in the response. This property is omitted for notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC 2.0 identifier, which can be a string, number, or null.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum JSONRPCId {
    String(String),
    Integer(i64),
    Null,
}

/// Represents a JSON-RPC 2.0 Request object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCRequest {
    /// The version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: String,
    /// A string containing the name of the method to be invoked.
    pub method: String,
    /// A structured value holding the parameter values to be used during the method invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    /// A unique identifier established by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<JSONRPCId>,
}

/// Represents a successful JSON-RPC 2.0 Response object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCSuccessResponse {
    /// The version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: String,
    /// The value of this member is determined by the method invoked on the Server.
    pub result: serde_json::Value,
    /// The identifier established by the client.
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC 2.0 Error Response object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCErrorResponse {
    /// The version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: String,
    /// An object describing the error that occurred.
    pub error: JSONRPCError,
    /// The identifier established by the client.
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC 2.0 Error object, included in an error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCError {
    /// A number that indicates the error type that occurred.
    pub code: i32,
    /// A string providing a short description of the error.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    /// This may be omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// A discriminated union representing all possible JSON-RPC 2.0 responses
/// for the A2A specification methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JSONRPCResponse {
    SendMessage(SendMessageResponse),
    SendStreamingMessage(SendStreamingMessageResponse),
    GetTask(GetTaskResponse),
    CancelTask(CancelTaskResponse),
    SetTaskPushNotificationConfig(SetTaskPushNotificationConfigResponse),
    GetTaskPushNotificationConfig(GetTaskPushNotificationConfigResponse),
    ListTaskPushNotificationConfig(ListTaskPushNotificationConfigResponse),
    DeleteTaskPushNotificationConfig(DeleteTaskPushNotificationConfigResponse),
    GetAuthenticatedExtendedCard(GetAuthenticatedExtendedCardResponse),
    Error(JSONRPCErrorResponse),
}

// ============================================================================
// A2A Error Types (from schema definitions)
// ============================================================================

/// An error indicating that the server received invalid JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct JSONParseError {
    /// The error code for a JSON parse error.
    pub code: i32, // -32700
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for JSONParseError {
    fn default() -> Self {
        Self {
            code: JSON_PARSE_ERROR_CODE,
            message: JSON_PARSE_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An error indicating that the JSON sent is not a valid Request object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InvalidRequestError {
    /// The error code for an invalid request.
    pub code: i32, // -32600
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for InvalidRequestError {
    fn default() -> Self {
        Self {
            code: INVALID_REQUEST_ERROR_CODE,
            message: INVALID_REQUEST_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An error indicating that the requested method does not exist or is not available.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MethodNotFoundError {
    /// The error code for a method not found error.
    pub code: i32, // -32601
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for MethodNotFoundError {
    fn default() -> Self {
        Self {
            code: METHOD_NOT_FOUND_ERROR_CODE,
            message: METHOD_NOT_FOUND_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An error indicating that the method parameters are invalid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InvalidParamsError {
    /// The error code for an invalid parameters error.
    pub code: i32, // -32602
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for InvalidParamsError {
    fn default() -> Self {
        Self {
            code: INVALID_PARAMS_ERROR_CODE,
            message: INVALID_PARAMS_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An error indicating an internal error on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InternalError {
    /// The error code for an internal server error.
    pub code: i32, // -32603
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for InternalError {
    fn default() -> Self {
        Self {
            code: INTERNAL_ERROR_CODE,
            message: INTERNAL_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An A2A-specific error indicating that the requested task ID was not found.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskNotFoundError {
    /// The error code for a task not found error.
    pub code: i32, // -32001
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for TaskNotFoundError {
    fn default() -> Self {
        Self {
            code: TASK_NOT_FOUND_ERROR_CODE,
            message: TASK_NOT_FOUND_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An A2A-specific error indicating that the task is in a state where it cannot be canceled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskNotCancelableError {
    /// The error code for a task that cannot be canceled.
    pub code: i32, // -32002
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for TaskNotCancelableError {
    fn default() -> Self {
        Self {
            code: TASK_NOT_CANCELABLE_ERROR_CODE,
            message: TASK_NOT_CANCELABLE_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An A2A-specific error indicating that the agent does not support push notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PushNotificationNotSupportedError {
    /// The error code for when push notifications are not supported.
    pub code: i32, // -32003
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for PushNotificationNotSupportedError {
    fn default() -> Self {
        Self {
            code: PUSH_NOTIFICATION_NOT_SUPPORTED_ERROR_CODE,
            message: PUSH_NOTIFICATION_NOT_SUPPORTED_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An A2A-specific error indicating that the requested operation is not supported by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UnsupportedOperationError {
    /// The error code for an unsupported operation.
    pub code: i32, // -32004
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for UnsupportedOperationError {
    fn default() -> Self {
        Self {
            code: UNSUPPORTED_OPERATION_ERROR_CODE,
            message: UNSUPPORTED_OPERATION_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An A2A-specific error indicating an incompatibility between the requested
/// content types and the agent's capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentTypeNotSupportedError {
    /// The error code for an unsupported content type.
    pub code: i32, // -32005
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for ContentTypeNotSupportedError {
    fn default() -> Self {
        Self {
            code: CONTENT_TYPE_NOT_SUPPORTED_ERROR_CODE,
            message: CONTENT_TYPE_NOT_SUPPORTED_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An A2A-specific error indicating that the agent returned a response that
/// does not conform to the specification for the current method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InvalidAgentResponseError {
    /// The error code for an invalid agent response.
    pub code: i32, // -32006
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for InvalidAgentResponseError {
    fn default() -> Self {
        Self {
            code: INVALID_AGENT_RESPONSE_ERROR_CODE,
            message: INVALID_AGENT_RESPONSE_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// An A2A-specific error indicating that the agent does not have an Authenticated Extended Card configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthenticatedExtendedCardNotConfiguredError {
    /// The error code for when an authenticated extended card is not configured.
    pub code: i32, // -32007
    /// The error message.
    pub message: String,
    /// A primitive or structured value containing additional information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Default for AuthenticatedExtendedCardNotConfiguredError {
    fn default() -> Self {
        Self {
            code: AUTHENTICATED_EXTENDED_CARD_NOT_CONFIGURED_ERROR_CODE,
            message: AUTHENTICATED_EXTENDED_CARD_NOT_CONFIGURED_ERROR_MESSAGE.to_string(),
            data: None,
        }
    }
}

/// A discriminated union of all standard JSON-RPC and A2A-specific error types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum A2AError {
    JSONParse(JSONParseError),
    InvalidRequest(InvalidRequestError),
    MethodNotFound(MethodNotFoundError),
    InvalidParams(InvalidParamsError),
    Internal(InternalError),
    TaskNotFound(TaskNotFoundError),
    TaskNotCancelable(TaskNotCancelableError),
    PushNotificationNotSupported(PushNotificationNotSupportedError),
    UnsupportedOperation(UnsupportedOperationError),
    ContentTypeNotSupported(ContentTypeNotSupportedError),
    InvalidAgentResponse(InvalidAgentResponseError),
    AuthenticatedExtendedCardNotConfigured(AuthenticatedExtendedCardNotConfiguredError),
}

// Error code and message constants
const JSON_PARSE_ERROR_CODE: i32 = -32700;
const JSON_PARSE_ERROR_MESSAGE: &str = "Invalid JSON payload";
const INVALID_REQUEST_ERROR_CODE: i32 = -32600;
const INVALID_REQUEST_ERROR_MESSAGE: &str = "Request payload validation error";
const METHOD_NOT_FOUND_ERROR_CODE: i32 = -32601;
const METHOD_NOT_FOUND_ERROR_MESSAGE: &str = "Method not found";
const INVALID_PARAMS_ERROR_CODE: i32 = -32602;
const INVALID_PARAMS_ERROR_MESSAGE: &str = "Invalid parameters";
const INTERNAL_ERROR_CODE: i32 = -32603;
const INTERNAL_ERROR_MESSAGE: &str = "Internal error";
const TASK_NOT_FOUND_ERROR_CODE: i32 = -32001;
const TASK_NOT_FOUND_ERROR_MESSAGE: &str = "Task not found";
const TASK_NOT_CANCELABLE_ERROR_CODE: i32 = -32002;
const TASK_NOT_CANCELABLE_ERROR_MESSAGE: &str = "Task cannot be canceled";
const PUSH_NOTIFICATION_NOT_SUPPORTED_ERROR_CODE: i32 = -32003;
const PUSH_NOTIFICATION_NOT_SUPPORTED_ERROR_MESSAGE: &str = "Push Notification is not supported";
const UNSUPPORTED_OPERATION_ERROR_CODE: i32 = -32004;
const UNSUPPORTED_OPERATION_ERROR_MESSAGE: &str = "This operation is not supported";
const CONTENT_TYPE_NOT_SUPPORTED_ERROR_CODE: i32 = -32005;
const CONTENT_TYPE_NOT_SUPPORTED_ERROR_MESSAGE: &str = "Incompatible content types";
const INVALID_AGENT_RESPONSE_ERROR_CODE: i32 = -32006;
const INVALID_AGENT_RESPONSE_ERROR_MESSAGE: &str = "Invalid agent response";
const AUTHENTICATED_EXTENDED_CARD_NOT_CONFIGURED_ERROR_CODE: i32 = -32007;
const AUTHENTICATED_EXTENDED_CARD_NOT_CONFIGURED_ERROR_MESSAGE: &str =
    "Authenticated Extended Card is not configured";

// ============================================================================
// A2A Core Protocol Types (from schema)
// ============================================================================

/// Defines the lifecycle states of a Task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    /// The task has been submitted and is awaiting execution.
    Submitted,
    /// The agent is actively working on the task.
    Working,
    /// The task is paused and waiting for input from the user.
    InputRequired,
    /// The task has been successfully completed.
    Completed,
    /// The task has been canceled by the user.
    Canceled,
    /// The task failed due to an error during execution.
    Failed,
    /// The task was rejected by the agent and was not started.
    Rejected,
    /// The task requires authentication to proceed.
    AuthRequired,
    /// The task is in an unknown or indeterminate state.
    Unknown,
}

/// Represents the status of a task at a specific point in time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatus {
    /// The current state of the task's lifecycle.
    pub state: TaskState,
    /// An ISO 8601 datetime string indicating when this status was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// An optional, human-readable message providing more details about the current status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
}

/// Represents a single, stateful operation or conversation between a client and an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    /// The type of this object, used as a discriminator. Always 'task'.
    #[serde(default = "default_task_kind")]
    pub kind: String,
    /// A unique identifier for the task, generated by the server for a new task.
    pub id: String,
    /// A server-generated identifier for maintaining context across multiple related tasks or interactions.
    #[serde(rename = "contextId")]
    pub context_id: String,
    /// The current status of the task, including its state and a descriptive message.
    pub status: TaskStatus,
    /// An array of messages exchanged during the task, representing the conversation history.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub history: Vec<Message>,
    /// A collection of artifacts generated by the agent during the execution of the task.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub artifacts: Vec<Artifact>,
    /// Optional metadata for extensions. The key is an extension-specific identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_task_kind() -> String {
    TASK_KIND.to_string()
}

/// Identifies the sender of a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// For messages sent by the client/user.
    User,
    /// For messages sent by the agent/service.
    Agent,
}

impl PartialEq<&str> for MessageRole {
    fn eq(&self, other: &&str) -> bool {
        matches!(
            (self, *other),
            (MessageRole::User, "user")
                | (MessageRole::Agent, "agent")
                | (MessageRole::Agent, "assistant")
        )
    }
}

/// Represents a single message in the conversation between a user and an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// The type of this object, used as a discriminator. Always 'message'.
    #[serde(default = "default_message_kind")]
    pub kind: String,
    /// A unique identifier for the message, typically a UUID, generated by the sender.
    #[serde(rename = "messageId")]
    pub message_id: String,
    /// Identifies the sender of the message. `user` for the client, `agent` for the service.
    pub role: MessageRole,
    /// An array of content parts that form the message body.
    pub parts: Vec<Part>,
    /// The context identifier for this message, used to group related interactions.
    #[serde(skip_serializing_if = "Option::is_none", rename = "contextId")]
    pub context_id: Option<String>,
    /// The identifier of the task this message is part of. Can be omitted for the first message of a new task.
    #[serde(skip_serializing_if = "Option::is_none", rename = "taskId")]
    pub task_id: Option<String>,
    /// A list of other task IDs that this message references for additional context.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        rename = "referenceTaskIds",
        default
    )]
    pub reference_task_ids: Vec<String>,
    /// The URIs of extensions that are relevant to this message.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extensions: Vec<String>,
    /// Optional metadata for extensions. The key is an extension-specific identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_message_kind() -> String {
    MESSAGE_KIND.to_string()
}

/// A discriminated union representing a part of a message or artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    /// Represents a text segment.
    Text {
        /// The string content of the text part.
        text: String,
        /// Optional metadata associated with this part.
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, serde_json::Value>>,
    },
    /// Represents a file segment.
    File {
        /// The file content, represented as either a URI or as base64-encoded bytes.
        file: FileContent,
        /// Optional metadata associated with this part.
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, serde_json::Value>>,
    },
    /// Represents a structured data segment (e.g., JSON).
    Data {
        /// The structured data content.
        data: serde_json::Value,
        /// Optional metadata associated with this part.
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, serde_json::Value>>,
    },
}

impl Part {
    pub fn as_data(&self) -> Option<&serde_json::Value> {
        match self {
            Part::Data { data, .. } => Some(data),
            _ => None,
        }
    }
}

/// Represents file content, which can be provided either directly as bytes or as a URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FileContent {
    WithBytes(FileWithBytes),
    WithUri(FileWithUri),
}

/// Represents a file with its content provided directly as a base64-encoded string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWithBytes {
    /// The base64-encoded content of the file.
    pub bytes: String,
    /// The MIME type of the file (e.g., "application/pdf").
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
    /// An optional name for the file (e.g., "document.pdf").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Represents a file with its content located at a specific URI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWithUri {
    /// A URL pointing to the file's content.
    pub uri: String,
    /// The MIME type of the file (e.g., "application/pdf").
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
    /// An optional name for the file (e.g., "document.pdf").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Represents a file, data structure, or other resource generated by an agent during a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    /// A unique identifier for the artifact within the scope of the task.
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    /// An array of content parts that make up the artifact.
    pub parts: Vec<Part>,
    /// An optional, human-readable name for the artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An optional, human-readable description of the artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The URIs of extensions that are relevant to this artifact.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extensions: Vec<String>,
    /// Optional metadata for extensions. The key is an extension-specific identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// A2A Method Parameter Types (from schema)
// ============================================================================

/// Defines the parameters for a request to send a message to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSendParams {
    /// The message object being sent to the agent.
    pub message: Message,
    /// Optional configuration for the send request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageSendConfiguration>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Defines configuration options for a `message/send` or `message/stream` request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageSendConfiguration {
    /// If true, the client will wait for the task to complete. The server may reject this if the task is long-running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
    /// The number of most recent messages from the task's history to retrieve in the response.
    #[serde(skip_serializing_if = "Option::is_none", rename = "historyLength")]
    pub history_length: Option<i32>,
    /// A list of output MIME types the client is prepared to accept in the response.
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        rename = "acceptedOutputModes",
        default
    )]
    pub accepted_output_modes: Vec<String>,
    /// Configuration for the agent to send push notifications for updates after the initial response.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "pushNotificationConfig"
    )]
    pub push_notification_config: Option<PushNotificationConfig>,
}

/// Defines the configuration for setting up push notifications for task updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationConfig {
    /// The callback URL where the agent should send push notifications.
    pub url: String,
    /// A unique ID for the push notification configuration, set by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// A unique token for this task or session to validate incoming push notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Optional authentication details for the agent to use when calling the notification URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<PushNotificationAuthenticationInfo>,
}

/// Defines authentication details for a push notification endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationAuthenticationInfo {
    /// A list of supported authentication schemes (e.g., 'Basic', 'Bearer').
    pub schemes: Vec<String>,
    /// Optional credentials required by the push notification endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,
}

/// Defines parameters containing a task ID, used for simple task operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Defines parameters for querying a task, with an option to limit history length.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueryParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The number of most recent messages from the task's history to retrieve.
    #[serde(skip_serializing_if = "Option::is_none", rename = "historyLength")]
    pub history_length: Option<i32>,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// A container associating a push notification configuration with a specific task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPushNotificationConfig {
    /// The ID of the task.
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// The push notification configuration for this task.
    #[serde(rename = "pushNotificationConfig")]
    pub push_notification_config: PushNotificationConfig,
}

/// Defines parameters for fetching a specific push notification configuration for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetTaskPushNotificationConfigParams {
    TaskIdOnly(TaskIdParams),
    WithConfigId(GetTaskPushNotificationConfigParamsWithId),
}

/// Parameters for fetching a push notification configuration with a specific config ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPushNotificationConfigParamsWithId {
    /// The unique identifier of the task.
    pub id: String,
    /// The ID of the push notification configuration to retrieve.
    #[serde(rename = "pushNotificationConfigId")]
    pub push_notification_config_id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Defines parameters for listing all push notification configurations associated with a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushNotificationConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Defines parameters for deleting a specific push notification configuration for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskPushNotificationConfigParams {
    /// The unique identifier of the task.
    pub id: String,
    /// The ID of the push notification configuration to delete.
    #[serde(rename = "pushNotificationConfigId")]
    pub push_notification_config_id: String,
    /// Optional metadata associated with the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// A2A Request Types (from schema)
// ============================================================================

fn default_jsonrpc_version() -> String {
    "2.0".to_string()
}

/// Represents a complete A2A JSON-RPC request, wrapping the payload with common fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ARequest {
    /// The JSON-RPC version. Always "2.0".
    #[serde(default = "default_jsonrpc_version")]
    pub jsonrpc: String,
    /// The request identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<JSONRPCId>,
    /// The specific A2A method and its parameters.
    #[serde(flatten)]
    pub payload: A2ARequestPayload,
}

/// A discriminated union of all possible A2A request payloads, tagged by the `method` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum A2ARequestPayload {
    /// Payload for the `message/send` method.
    #[serde(rename = "message/send")]
    SendMessage { params: MessageSendParams },
    /// Payload for the `message/stream` method.
    #[serde(rename = "message/stream")]
    SendStreamingMessage { params: MessageSendParams },
    /// Payload for the `tasks/get` method.
    #[serde(rename = "tasks/get")]
    GetTask { params: TaskQueryParams },
    /// Payload for the `tasks/cancel` method.
    #[serde(rename = "tasks/cancel")]
    CancelTask { params: TaskIdParams },
    /// Payload for the `tasks/pushNotificationConfig/set` method.
    #[serde(rename = "tasks/pushNotificationConfig/set")]
    SetTaskPushNotificationConfig { params: TaskPushNotificationConfig },
    /// Payload for the `tasks/pushNotificationConfig/get` method.
    #[serde(rename = "tasks/pushNotificationConfig/get")]
    GetTaskPushNotificationConfig {
        params: GetTaskPushNotificationConfigParams,
    },
    /// Payload for the `tasks/resubscribe` method.
    #[serde(rename = "tasks/resubscribe")]
    TaskResubscription { params: TaskIdParams },
    /// Payload for the `tasks/pushNotificationConfig/list` method.
    #[serde(rename = "tasks/pushNotificationConfig/list")]
    ListTaskPushNotificationConfig {
        params: ListTaskPushNotificationConfigParams,
    },
    /// Payload for the `tasks/pushNotificationConfig/delete` method.
    #[serde(rename = "tasks/pushNotificationConfig/delete")]
    DeleteTaskPushNotificationConfig {
        params: DeleteTaskPushNotificationConfigParams,
    },
    /// Payload for the `agent/getAuthenticatedExtendedCard` method.
    #[serde(rename = "agent/getAuthenticatedExtendedCard")]
    GetAuthenticatedExtendedCard,
}

// ============================================================================
// A2A Response Types (from schema)
// ============================================================================

/// The result of a `message/send` call, which can be a direct reply or a task object.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendMessageResult {
    Task(Task),
    Message(Message),
}

/// Represents a successful JSON-RPC response for the `message/send` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: SendMessageResult,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `message/send` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendMessageResponse {
    Success(Box<SendMessageSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// The result of a `message/stream` call, which can be an initial object or a streaming event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendStreamingMessageResult {
    Task(Task),
    Message(Message),
    TaskStatusUpdate(TaskStatusUpdateEvent),
    TaskArtifactUpdate(TaskArtifactUpdateEvent),
}

/// Represents a successful JSON-RPC response for the `message/stream` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendStreamingMessageSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: SendStreamingMessageResult,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `message/stream` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendStreamingMessageResponse {
    Success(Box<SendStreamingMessageSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// Represents a successful JSON-RPC response for the `tasks/get` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: Task,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `tasks/get` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetTaskResponse {
    Success(Box<GetTaskSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// Represents a successful JSON-RPC response for the `tasks/cancel` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: Task,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `tasks/cancel` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CancelTaskResponse {
    Success(Box<CancelTaskSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// Represents a successful JSON-RPC response for the `tasks/pushNotificationConfig/set` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: TaskPushNotificationConfig,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `tasks/pushNotificationConfig/set` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SetTaskPushNotificationConfigResponse {
    Success(Box<SetTaskPushNotificationConfigSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// Represents a successful JSON-RPC response for the `tasks/pushNotificationConfig/get` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: TaskPushNotificationConfig,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `tasks/pushNotificationConfig/get` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetTaskPushNotificationConfigResponse {
    Success(Box<GetTaskPushNotificationConfigSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// Represents a successful JSON-RPC response for the `tasks/pushNotificationConfig/list` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: Vec<TaskPushNotificationConfig>,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `tasks/pushNotificationConfig/list` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ListTaskPushNotificationConfigResponse {
    Success(Box<ListTaskPushNotificationConfigSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// Represents a successful JSON-RPC response for the `tasks/pushNotificationConfig/delete` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    /// The result is null on successful deletion.
    pub result: (),
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `tasks/pushNotificationConfig/delete` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DeleteTaskPushNotificationConfigResponse {
    Success(Box<DeleteTaskPushNotificationConfigSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

/// Represents a successful JSON-RPC response for the `agent/getAuthenticatedExtendedCard` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAuthenticatedExtendedCardSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: AgentCard,
    pub id: Option<JSONRPCId>,
}

/// Represents a JSON-RPC response for the `agent/getAuthenticatedExtendedCard` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetAuthenticatedExtendedCardResponse {
    Success(Box<GetAuthenticatedExtendedCardSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

// Constants for type values
pub const PROTOCOL_VERSION: &str = "0.3.0";
pub const API_KEY_TYPE: &str = "apiKey";
pub const HTTP_TYPE: &str = "http";
pub const OAUTH2_TYPE: &str = "oauth2";
pub const OPENID_TYPE: &str = "openIdConnect";
pub const MUTUAL_TLS_TYPE: &str = "mutualTLS";
pub const TASK_KIND: &str = "task";
pub const MESSAGE_KIND: &str = "message";
pub const STATUS_UPDATE_KIND: &str = "status-update";
pub const ARTIFACT_UPDATE_KIND: &str = "artifact-update";

// ============================================================================
// Security and Authentication Types (from schema)
// ============================================================================

/// Defines a security scheme that can be used to secure an agent's endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum SecurityScheme {
    ApiKey(APIKeySecurityScheme),
    Http(HTTPAuthSecurityScheme),
    OAuth2(Box<OAuth2SecurityScheme>),
    OpenIdConnect(OpenIdConnectSecurityScheme),
    MutualTLS(MutualTLSSecurityScheme),
}

/// Defines a security scheme using an API key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct APIKeySecurityScheme {
    /// The type of the security scheme. Must be 'apiKey'.
    #[serde(rename = "type", default = "default_api_key_type")]
    pub scheme_type: String,
    /// The name of the header, query, or cookie parameter to be used.
    pub name: String,
    /// The location of the API key.
    #[serde(rename = "in")]
    pub location: APIKeyLocation,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_api_key_type() -> String {
    API_KEY_TYPE.to_string()
}

/// The location of an API key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum APIKeyLocation {
    Query,
    Header,
    Cookie,
}

/// Defines a security scheme using HTTP authentication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HTTPAuthSecurityScheme {
    /// The type of the security scheme. Must be 'http'.
    #[serde(rename = "type", default = "default_http_type")]
    pub scheme_type: String,
    /// The name of the HTTP Authentication scheme to be used.
    pub scheme: String,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A hint to the client to identify how the bearer token is formatted.
    #[serde(skip_serializing_if = "Option::is_none", rename = "bearerFormat")]
    pub bearer_format: Option<String>,
}

fn default_http_type() -> String {
    HTTP_TYPE.to_string()
}

/// Defines a security scheme using OAuth 2.0.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuth2SecurityScheme {
    /// The type of the security scheme. Must be 'oauth2'.
    #[serde(rename = "type", default = "default_oauth2_type")]
    pub scheme_type: String,
    /// Configuration information for the supported OAuth 2.0 flows.
    pub flows: OAuthFlows,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// URL to the oauth2 authorization server metadata.
    #[serde(skip_serializing_if = "Option::is_none", rename = "oauth2MetadataUrl")]
    pub oauth2_metadata_url: Option<String>,
}

fn default_oauth2_type() -> String {
    OAUTH2_TYPE.to_string()
}

/// Defines a security scheme using OpenID Connect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenIdConnectSecurityScheme {
    /// The type of the security scheme. Must be 'openIdConnect'.
    #[serde(rename = "type", default = "default_openid_type")]
    pub scheme_type: String,
    /// The OpenID Connect Discovery URL.
    #[serde(rename = "openIdConnectUrl")]
    pub open_id_connect_url: String,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_openid_type() -> String {
    OPENID_TYPE.to_string()
}

/// Defines a security scheme using mTLS authentication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MutualTLSSecurityScheme {
    /// The type of the security scheme. Must be 'mutualTLS'.
    #[serde(rename = "type", default = "default_mutual_tls_type")]
    pub scheme_type: String,
    /// An optional description for the security scheme.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_mutual_tls_type() -> String {
    MUTUAL_TLS_TYPE.to_string()
}

/// Defines the configuration for the supported OAuth 2.0 flows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthFlows {
    /// Configuration for the OAuth Authorization Code flow.
    #[serde(skip_serializing_if = "Option::is_none", rename = "authorizationCode")]
    pub authorization_code: Option<AuthorizationCodeOAuthFlow>,
    /// Configuration for the OAuth Implicit flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<ImplicitOAuthFlow>,
    /// Configuration for the OAuth Resource Owner Password flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordOAuthFlow>,
    /// Configuration for the OAuth Client Credentials flow.
    #[serde(skip_serializing_if = "Option::is_none", rename = "clientCredentials")]
    pub client_credentials: Option<ClientCredentialsOAuthFlow>,
}

/// Defines configuration details for the OAuth 2.0 Authorization Code flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationCodeOAuthFlow {
    /// The authorization URL to be used for this flow.
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    /// The token URL to be used for this flow.
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

/// Defines configuration details for the OAuth 2.0 Implicit flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImplicitOAuthFlow {
    /// The authorization URL to be used for this flow.
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

/// Defines configuration details for the OAuth 2.0 Resource Owner Password flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordOAuthFlow {
    /// The token URL to be used for this flow.
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

/// Defines configuration details for the OAuth 2.0 Client Credentials flow.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientCredentialsOAuthFlow {
    /// The token URL to be used for this flow.
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    /// The available scopes for the OAuth2 security scheme.
    pub scopes: HashMap<String, String>,
    /// The URL to be used for obtaining refresh tokens.
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

// ============================================================================
// Convenience Types
// ============================================================================

/// Main agent response type that can be either a Task or Message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentResponse {
    Task(Task),
    Message(Message),
}

// ============================================================================
// Streaming Event Types (from schema)
// ============================================================================

/// An event sent by the agent to notify the client of a change in a task's status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatusUpdateEvent {
    /// The type of this event. Always "status-update".
    #[serde(default = "default_status_update_kind")]
    pub kind: String,
    /// The ID of the task that was updated.
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// The context ID associated with the task.
    #[serde(rename = "contextId")]
    pub context_id: String,
    /// The new status of the task.
    pub status: TaskStatus,
    /// If true, this is the final event in the stream for this interaction.
    #[serde(rename = "final")]
    pub is_final: bool,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_status_update_kind() -> String {
    STATUS_UPDATE_KIND.to_string()
}

/// An event sent by the agent to notify the client that an artifact has been generated or updated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskArtifactUpdateEvent {
    /// The type of this event. Always "artifact-update".
    #[serde(default = "default_artifact_update_kind")]
    pub kind: String,
    /// The ID of the task this artifact belongs to.
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// The context ID associated with the task.
    #[serde(rename = "contextId")]
    pub context_id: String,
    /// The artifact that was generated or updated.
    pub artifact: Artifact,
    /// If true, the content of this artifact should be appended to a previously sent artifact with the same ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append: Option<bool>,
    /// If true, this is the final chunk of the artifact.
    #[serde(skip_serializing_if = "Option::is_none", rename = "lastChunk")]
    pub last_chunk: Option<bool>,
    /// Optional metadata for extensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

fn default_artifact_update_kind() -> String {
    ARTIFACT_UPDATE_KIND.to_string()
}
