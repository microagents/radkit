//! Utility functions for working with models and A2A protocol types.
//!
//! This module provides helper functions for common conversions and operations
//! on model types, particularly for interop with the A2A protocol.

use a2a_types::{Message, MessageRole};
use uuid::Uuid;

use crate::agent::Artifact;
use crate::models::{Content, ContentPart, Role};

/// Creates an A2A Message from radkit Content.
///
/// This is a utility function for converting radkit's internal `Content` type
/// into the A2A protocol's `Message` type. It handles the conversion of content
/// parts and sets appropriate message metadata.
///
/// # Arguments
///
/// * `context_id` - Optional context ID for grouping related messages
/// * `task_id` - Optional task ID if this message is part of a task
/// * `role` - The role of the message sender (User, Assistant, System, or Tool)
/// * `content` - The message content to convert
///
/// # Examples
///
/// ```ignore
/// use radkit::models::{Content, Role, utils};
///
/// let content = Content::from_text("Hello, world!");
/// let message = utils::create_a2a_message(
///     Some("ctx-123"),
///     None,
///     Role::User,
///     content,
/// );
/// ```
pub fn create_a2a_message(
    context_id: Option<&str>,
    task_id: Option<&str>,
    role: Role,
    content: Content,
) -> Message {
    // Convert ContentParts to A2A Parts
    let parts: Vec<a2a_types::Part> = content
        .into_parts()
        .into_iter()
        .filter_map(ContentPart::into_a2a_part)
        .collect();

    // Map radkit Role to A2A MessageRole
    let message_role = match role {
        Role::User => MessageRole::User,
        Role::Assistant | Role::System | Role::Tool => MessageRole::Agent,
    };

    Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: message_role,
        context_id: context_id.map(std::string::ToString::to_string),
        task_id: task_id.map(std::string::ToString::to_string),
        parts,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    }
}

/// Converts radkit Artifact to A2A Artifact.
///
/// This is a utility function for converting radkit's internal `Artifact` type
/// into the A2A protocol's `Artifact` type. It handles the conversion of content
/// to parts and sets appropriate artifact metadata.
///
/// # Arguments
///
/// * `artifact` - The internal artifact to convert
///
/// # Examples
///
/// ```ignore
/// use radkit::agent::Artifact;
/// use radkit::models::utils;
///
/// let artifact = Artifact::from_json("result.json", &data)?;
/// let a2a_artifact = utils::artifact_to_a2a(artifact);
/// ```
pub fn artifact_to_a2a(artifact: Artifact) -> a2a_types::Artifact {
    // Convert Content to A2A Parts
    let parts: Vec<a2a_types::Part> = artifact
        .content()
        .clone()
        .into_parts()
        .into_iter()
        .filter_map(ContentPart::into_a2a_part)
        .collect();

    a2a_types::Artifact {
        artifact_id: artifact.name().to_string(),
        parts,
        name: Some(artifact.name().to_string()),
        description: None,
        extensions: Vec::new(),
        metadata: None,
    }
}

/// Converts a Vec of radkit Artifacts to A2A Artifacts.
///
/// # Arguments
///
/// * `artifacts` - Vector of internal artifacts to convert
pub fn artifacts_to_a2a(artifacts: Vec<Artifact>) -> Vec<a2a_types::Artifact> {
    artifacts.into_iter().map(artifact_to_a2a).collect()
}
