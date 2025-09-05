use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A2A Protocol Types
/// Based on A2A Protocol Specification JSON Schema
/// Only includes types explicitly defined in a2a.json

// ============================================================================
// JSON-RPC 2.0 Base Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCMessage {
    pub jsonrpc: String, // Always "2.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JSONRPCId {
    String(String),
    Integer(i64),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: serde_json::Value,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCErrorResponse {
    pub jsonrpc: String, // Always "2.0"
    pub error: JSONRPCError,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JSONRPCResponse {
    Success(JSONRPCSuccessResponse),
    Error(JSONRPCErrorResponse),
}

// ============================================================================
// A2A Error Types (from schema definitions)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONParseError {
    #[serde(default = "default_json_parse_error_code")]
    pub code: i32, // -32700
    #[serde(default = "default_json_parse_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidRequestError {
    #[serde(default = "default_invalid_request_error_code")]
    pub code: i32, // -32600
    #[serde(default = "default_invalid_request_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodNotFoundError {
    #[serde(default = "default_method_not_found_error_code")]
    pub code: i32, // -32601
    #[serde(default = "default_method_not_found_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidParamsError {
    #[serde(default = "default_invalid_params_error_code")]
    pub code: i32, // -32602
    #[serde(default = "default_invalid_params_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalError {
    #[serde(default = "default_internal_error_code")]
    pub code: i32, // -32603
    #[serde(default = "default_internal_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNotFoundError {
    #[serde(default = "default_task_not_found_error_code")]
    pub code: i32, // -32001
    #[serde(default = "default_task_not_found_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNotCancelableError {
    #[serde(default = "default_task_not_cancelable_error_code")]
    pub code: i32, // -32002
    #[serde(default = "default_task_not_cancelable_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationNotSupportedError {
    #[serde(default = "default_push_notification_not_supported_error_code")]
    pub code: i32, // -32003
    #[serde(default = "default_push_notification_not_supported_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsupportedOperationError {
    #[serde(default = "default_unsupported_operation_error_code")]
    pub code: i32, // -32004
    #[serde(default = "default_unsupported_operation_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentTypeNotSupportedError {
    #[serde(default = "default_content_type_not_supported_error_code")]
    pub code: i32, // -32005
    #[serde(default = "default_content_type_not_supported_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidAgentResponseError {
    #[serde(default = "default_invalid_agent_response_error_code")]
    pub code: i32, // -32006
    #[serde(default = "default_invalid_agent_response_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedExtendedCardNotConfiguredError {
    #[serde(default = "default_authenticated_extended_card_not_configured_error_code")]
    pub code: i32, // -32007
    #[serde(default = "default_authenticated_extended_card_not_configured_error_message")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

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

// Default functions for error codes and messages
fn default_json_parse_error_code() -> i32 {
    -32700
}
fn default_json_parse_error_message() -> String {
    "Invalid JSON payload".to_string()
}
fn default_invalid_request_error_code() -> i32 {
    -32600
}
fn default_invalid_request_error_message() -> String {
    "Request payload validation error".to_string()
}
fn default_method_not_found_error_code() -> i32 {
    -32601
}
fn default_method_not_found_error_message() -> String {
    "Method not found".to_string()
}
fn default_invalid_params_error_code() -> i32 {
    -32602
}
fn default_invalid_params_error_message() -> String {
    "Invalid parameters".to_string()
}
fn default_internal_error_code() -> i32 {
    -32603
}
fn default_internal_error_message() -> String {
    "Internal error".to_string()
}
fn default_task_not_found_error_code() -> i32 {
    -32001
}
fn default_task_not_found_error_message() -> String {
    "Task not found".to_string()
}
fn default_task_not_cancelable_error_code() -> i32 {
    -32002
}
fn default_task_not_cancelable_error_message() -> String {
    "Task cannot be canceled".to_string()
}
fn default_push_notification_not_supported_error_code() -> i32 {
    -32003
}
fn default_push_notification_not_supported_error_message() -> String {
    "Push Notification is not supported".to_string()
}
fn default_unsupported_operation_error_code() -> i32 {
    -32004
}
fn default_unsupported_operation_error_message() -> String {
    "This operation is not supported".to_string()
}
fn default_content_type_not_supported_error_code() -> i32 {
    -32005
}
fn default_content_type_not_supported_error_message() -> String {
    "Incompatible content types".to_string()
}
fn default_invalid_agent_response_error_code() -> i32 {
    -32006
}
fn default_invalid_agent_response_error_message() -> String {
    "Invalid agent response".to_string()
}
fn default_authenticated_extended_card_not_configured_error_code() -> i32 {
    -32007
}
fn default_authenticated_extended_card_not_configured_error_message() -> String {
    "Authenticated Extended Card is not configured".to_string()
}

// ============================================================================
// A2A Core Protocol Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Canceled,
    Failed,
    Rejected,
    AuthRequired,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>, // ISO 8601 datetime
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub kind: String, // Always "task"
    pub id: String,
    #[serde(rename = "contextId")]
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<Artifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub kind: String, // Always "message"
    #[serde(rename = "messageId")]
    pub message_id: String,
    pub role: MessageRole,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "contextId")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "taskId")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "referenceTaskIds")]
    pub reference_task_ids: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, serde_json::Value>>,
    },
    File {
        file: FileContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        metadata: Option<HashMap<String, serde_json::Value>>,
    },
    Data {
        data: serde_json::Value,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum FileContent {
    WithBytes(FileWithBytes),
    WithUri(FileWithUri),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileBase {
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWithBytes {
    #[serde(flatten)]
    pub base: FileBase,
    pub bytes: String, // base64-encoded
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWithUri {
    #[serde(flatten)]
    pub base: FileBase,
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artifact {
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// A2A Method Parameter Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSendParams {
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageSendConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageSendConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "historyLength")]
    pub history_length: Option<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "acceptedOutputModes")]
    pub accepted_output_modes: Vec<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "pushNotificationConfig"
    )]
    pub push_notification_config: Option<PushNotificationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationConfig {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<PushNotificationAuthenticationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationAuthenticationInfo {
    pub schemes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIdParams {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueryParams {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "historyLength")]
    pub history_length: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPushNotificationConfig {
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "pushNotificationConfig")]
    pub push_notification_config: PushNotificationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetTaskPushNotificationConfigParams {
    TaskIdOnly(TaskIdParams),
    WithConfigId(GetTaskPushNotificationConfigParamsWithId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPushNotificationConfigParamsWithId {
    pub id: String,
    #[serde(rename = "pushNotificationConfigId")]
    pub push_notification_config_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushNotificationConfigParams {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskPushNotificationConfigParams {
    pub id: String,
    #[serde(rename = "pushNotificationConfigId")]
    pub push_notification_config_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ============================================================================
// A2A Request Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "message/send"
    pub params: MessageSendParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendStreamingMessageRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "message/stream"
    pub params: MessageSendParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "tasks/get"
    pub params: TaskQueryParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "tasks/cancel"
    pub params: TaskIdParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTaskPushNotificationConfigRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "tasks/pushNotificationConfig/set"
    pub params: TaskPushNotificationConfig,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPushNotificationConfigRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "tasks/pushNotificationConfig/get"
    pub params: GetTaskPushNotificationConfigParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushNotificationConfigRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "tasks/pushNotificationConfig/list"
    pub params: ListTaskPushNotificationConfigParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskPushNotificationConfigRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "tasks/pushNotificationConfig/delete"
    pub params: DeleteTaskPushNotificationConfigParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResubscriptionRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "tasks/resubscribe"
    pub params: TaskIdParams,
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAuthenticatedExtendedCardRequest {
    pub jsonrpc: String, // Always "2.0"
    pub method: String,  // Always "agent/getAuthenticatedExtendedCard"
    pub id: JSONRPCId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum A2ARequest {
    SendMessage(SendMessageRequest),
    SendStreamingMessage(SendStreamingMessageRequest),
    GetTask(GetTaskRequest),
    CancelTask(CancelTaskRequest),
    SetTaskPushNotificationConfig(SetTaskPushNotificationConfigRequest),
    GetTaskPushNotificationConfig(GetTaskPushNotificationConfigRequest),
    TaskResubscription(TaskResubscriptionRequest),
    ListTaskPushNotificationConfig(ListTaskPushNotificationConfigRequest),
    DeleteTaskPushNotificationConfig(DeleteTaskPushNotificationConfigRequest),
    GetAuthenticatedExtendedCard(GetAuthenticatedExtendedCardRequest),
}

// ============================================================================
// A2A Response Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendMessageResult {
    Task(Task),
    Message(Message),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: SendMessageResult,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendMessageResponse {
    Success(Box<SendMessageSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendStreamingMessageResult {
    Task(Task),
    Message(Message),
    TaskStatusUpdate(TaskStatusUpdateEvent),
    TaskArtifactUpdate(TaskArtifactUpdateEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendStreamingMessageSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: SendStreamingMessageResult,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SendStreamingMessageResponse {
    Success(Box<SendStreamingMessageSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: Task,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetTaskResponse {
    Success(Box<GetTaskSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: Task,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CancelTaskResponse {
    Success(Box<CancelTaskSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: TaskPushNotificationConfig,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SetTaskPushNotificationConfigResponse {
    Success(SetTaskPushNotificationConfigSuccessResponse),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: TaskPushNotificationConfig,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetTaskPushNotificationConfigResponse {
    Success(GetTaskPushNotificationConfigSuccessResponse),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: Vec<TaskPushNotificationConfig>,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ListTaskPushNotificationConfigResponse {
    Success(ListTaskPushNotificationConfigSuccessResponse),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTaskPushNotificationConfigSuccessResponse {
    pub jsonrpc: String,                   // Always "2.0"
    pub result: Option<serde_json::Value>, // null on success
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DeleteTaskPushNotificationConfigResponse {
    Success(DeleteTaskPushNotificationConfigSuccessResponse),
    Error(JSONRPCErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAuthenticatedExtendedCardSuccessResponse {
    pub jsonrpc: String, // Always "2.0"
    pub result: AgentCard,
    pub id: Option<JSONRPCId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetAuthenticatedExtendedCardResponse {
    Success(Box<GetAuthenticatedExtendedCardSuccessResponse>),
    Error(JSONRPCErrorResponse),
}

// ============================================================================
// A2A Agent Card and Discovery Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TransportProtocol {
    #[serde(rename = "JSONRPC")]
    JsonRpc,
    #[serde(rename = "GRPC")]
    Grpc,
    #[serde(rename = "HTTP+JSON")]
    HttpJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInterface {
    pub transport: TransportProtocol,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExtension {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "pushNotifications")]
    pub push_notifications: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "stateTransitionHistory"
    )]
    pub state_transition_history: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<AgentExtension>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProvider {
    pub organization: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "inputModes")]
    pub input_modes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "outputModes")]
    pub output_modes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<HashMap<String, Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardSignature {
    #[serde(rename = "protected")]
    pub protected_header: String, // Base64url-encoded
    pub signature: String, // Base64url-encoded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String, // defaults to "0.3.0"
    pub url: String,
    #[serde(rename = "preferredTransport")]
    pub preferred_transport: TransportProtocol, // defaults to JSONRPC
    pub capabilities: AgentCapabilities,
    #[serde(rename = "defaultInputModes")]
    pub default_input_modes: Vec<String>,
    #[serde(rename = "defaultOutputModes")]
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "additionalInterfaces")]
    pub additional_interfaces: Vec<AgentInterface>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "documentationUrl")]
    pub documentation_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "iconUrl")]
    pub icon_url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<HashMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "securitySchemes")]
    pub security_schemes: Option<HashMap<String, SecurityScheme>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<AgentCardSignature>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "supportsAuthenticatedExtendedCard"
    )]
    pub supports_authenticated_extended_card: Option<bool>,
}

// ============================================================================
// Security and Authentication Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SecurityScheme {
    #[serde(rename = "apiKey")]
    ApiKey(APIKeySecurityScheme),
    #[serde(rename = "http")]
    Http(HTTPAuthSecurityScheme),
    #[serde(rename = "oauth2")]
    OAuth2(Box<OAuth2SecurityScheme>),
    #[serde(rename = "openIdConnect")]
    OpenIdConnect(OpenIdConnectSecurityScheme),
    #[serde(rename = "mutualTLS")]
    MutualTLS(MutualTLSSecurityScheme),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIKeySecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String, // Always "apiKey"
    pub name: String,
    #[serde(rename = "in")]
    pub location: APIKeyLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum APIKeyLocation {
    Query,
    Header,
    Cookie,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HTTPAuthSecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String, // Always "http"
    pub scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "bearerFormat")]
    pub bearer_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2SecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String, // Always "oauth2"
    pub flows: OAuthFlows,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "oauth2MetadataUrl")]
    pub oauth2_metadata_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenIdConnectSecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String, // Always "openIdConnect"
    #[serde(rename = "openIdConnectUrl")]
    pub open_id_connect_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutualTLSSecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String, // Always "mutualTLS"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthFlows {
    #[serde(skip_serializing_if = "Option::is_none", rename = "authorizationCode")]
    pub authorization_code: Option<AuthorizationCodeOAuthFlow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit: Option<ImplicitOAuthFlow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordOAuthFlow>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "clientCredentials")]
    pub client_credentials: Option<ClientCredentialsOAuthFlow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationCodeOAuthFlow {
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    pub scopes: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplicitOAuthFlow {
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: String,
    pub scopes: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordOAuthFlow {
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    pub scopes: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCredentialsOAuthFlow {
    #[serde(rename = "tokenUrl")]
    pub token_url: String,
    pub scopes: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "refreshUrl")]
    pub refresh_url: Option<String>,
}

// ============================================================================
// Convenience Types
// ============================================================================

/// Main agent response type that can be either a Task or Message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentResponse {
    Task(Task),
    Message(Message),
}

// ============================================================================
// Streaming Event Types (from schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskStatusUpdateEvent {
    pub kind: String, // Always "status-update"
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "contextId")]
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(rename = "final")]
    pub is_final: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskArtifactUpdateEvent {
    pub kind: String, // Always "artifact-update"
    #[serde(rename = "taskId")]
    pub task_id: String,
    #[serde(rename = "contextId")]
    pub context_id: String,
    pub artifact: Artifact,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "lastChunk")]
    pub last_chunk: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}
