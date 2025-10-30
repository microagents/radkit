//! Events representing messages in a conversation thread.
//!
//! This module provides [`Event`] and [`Role`] types for modeling conversation
//! interactions between users, assistants, and tools in an LLM conversation.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::models::{Event, Role, Content};
//!
//! // Create a user message
//! let user_event = Event::user("What is 2+2?");
//!
//! // Create an assistant response
//! let assistant_event = Event::assistant("The answer is 4");
//! ```

use crate::models::content::Content;
use crate::tools::{ToolCall, ToolResponse};
use serde::{Deserialize, Serialize};

/// The role of a participant in a conversation.
///
/// Roles identify who or what is speaking in a conversation thread.
/// This enum is marked `#[non_exhaustive]` to allow adding new roles
/// (e.g., `Function`, `Developer`) without breaking changes.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize, derive_more::Display)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single event in a conversation thread.
///
/// An event represents a single message or interaction in a conversation,
/// with an associated role indicating who/what generated the content.
///
/// Use the constructor methods ([`Event::system`], [`Event::user`], [`Event::assistant`])
/// to create events, and the accessor methods ([`role`](Event::role), [`content`](Event::content))
/// to read the fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    role: Role,
    content: Content,
}

impl Event {
    /// Creates a new `Event` with a `System` role.
    ///
    /// System events typically contain instructions or context for the LLM.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::models::Event;
    ///
    /// let event = Event::system("You are a helpful assistant");
    /// ```
    pub fn system(content: impl Into<Content>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    /// Creates a new `Event` with an `Assistant` role.
    ///
    /// Assistant events represent messages from the LLM.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::models::Event;
    ///
    /// let event = Event::assistant("Hello! How can I help you?");
    /// ```
    pub fn assistant(content: impl Into<Content>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }

    /// Creates a new `Event` with a `User` role.
    ///
    /// User events represent messages from the end user.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::models::Event;
    ///
    /// let event = Event::user("What is the weather today?");
    /// ```
    pub fn user(content: impl Into<Content>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// Returns a reference to the role of this event.
    #[must_use] pub const fn role(&self) -> &Role {
        &self.role
    }

    /// Returns a reference to the content of this event.
    #[must_use] pub const fn content(&self) -> &Content {
        &self.content
    }

    /// Consumes the event and returns the content.
    #[must_use] pub fn into_content(self) -> Content {
        self.content
    }

    /// Consumes the event and returns both role and content.
    #[must_use] pub fn into_parts(self) -> (Role, Content) {
        (self.role, self.content)
    }
}

impl From<Vec<ToolCall>> for Event {
    fn from(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::from(tool_calls),
        }
    }
}

impl From<ToolResponse> for Event {
    fn from(value: ToolResponse) -> Self {
        Self {
            role: Role::Tool,
            content: Content::from(value),
        }
    }
}
