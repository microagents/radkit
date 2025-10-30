//! Content containers for message parts.
//!
//! This module provides the [`Content`] type, a container for multiple [`ContentPart`]s
//! that make up the content of a message. Content can include text, binary data,
//! tool calls, and tool responses in any combination.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::models::{Content, ContentPart};
//!
//! // Create content from text
//! let content = Content::from_text("Hello, world!");
//!
//! // Create content with multiple parts
//! let content = Content::from_parts(vec![
//!     ContentPart::Text("Check this image:".to_string()),
//!     ContentPart::from_data("image/png", "iVBORw0KG...", None).unwrap(),
//! ]);
//!
//! // Access text parts
//! for text in content.texts() {
//!     println!("{}", text);
//! }
//! ```

pub(crate) use crate::models::ContentPart;
use crate::tools::tool::{ToolCall, ToolResponse};
use serde::{Deserialize, Serialize};
use std::iter::FromIterator;
use std::slice::{Iter, IterMut};

/// A container for a list of content parts.
///
/// Content represents the payload of a message, which can include multiple parts
/// of different types (text, data, tool calls, tool responses). This flexible
/// structure allows rich, multi-modal messages.
///
/// The type implements common collection traits like [`IntoIterator`], [`Extend`],
/// and [`FromIterator`] for convenient manipulation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Content {
    parts: Vec<ContentPart>,
}

impl Content {
    /// Creates a new `Content` from a single text part.
    pub fn from_text(content: impl Into<String>) -> Self {
        Self {
            parts: vec![ContentPart::Text(content.into())],
        }
    }

    /// Creates a new `Content` from a vector of `ContentPart`s.
    pub fn from_parts(parts: impl Into<Vec<ContentPart>>) -> Self {
        Self {
            parts: parts.into(),
        }
    }

    /// Creates a new `Content` from a vector of `ToolCall`s.
    pub fn from_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            parts: tool_calls.into_iter().map(ContentPart::ToolCall).collect(),
        }
    }

    /// Appends a `ContentPart` to the content.
    pub fn append(mut self, part: impl Into<ContentPart>) -> Self {
        self.parts.push(part.into());
        self
    }

    /// Pushes a `ContentPart` to the content.
    pub fn push(&mut self, part: impl Into<ContentPart>) {
        self.parts.push(part.into());
    }

    /// Extends the content with an iterator of `ContentPart`s.
    pub fn extended<I>(mut self, iter: I) -> Self
    where
        I: IntoIterator<Item = ContentPart>,
    {
        self.parts.extend(iter);
        self
    }

    /// Returns a slice of the content parts.
    ///
    /// This method returns a slice rather than a reference to the internal `Vec`
    /// for better API flexibility and ergonomics.
    #[must_use] pub fn parts(&self) -> &[ContentPart] {
        &self.parts
    }

    /// Consumes the `Content` and returns the `ContentPart`s.
    #[must_use] pub fn into_parts(self) -> Vec<ContentPart> {
        self.parts
    }

    /// Returns all text parts as a vector of `&str`.
    #[must_use] pub fn texts(&self) -> Vec<&str> {
        self.parts.iter().filter_map(|p| p.as_text()).collect()
    }

    /// Consumes the `Content` and returns all text parts as a vector of `String`.
    #[must_use] pub fn into_texts(self) -> Vec<String> {
        self.parts
            .into_iter()
            .filter_map(super::content_part::ContentPart::into_text)
            .collect()
    }

    /// Returns all `ToolCall` parts as a vector of references.
    #[must_use] pub fn tool_calls(&self) -> Vec<&ToolCall> {
        self.parts.iter().filter_map(|p| p.as_tool_call()).collect()
    }

    /// Consumes the `Content` and returns all `ToolCall` parts.
    #[must_use] pub fn into_tool_calls(self) -> Vec<ToolCall> {
        self.parts
            .into_iter()
            .filter_map(super::content_part::ContentPart::into_tool_call)
            .collect()
    }

    /// Returns all `ToolResponse` parts as a vector of references.
    #[must_use] pub fn tool_responses(&self) -> Vec<&ToolResponse> {
        self.parts
            .iter()
            .filter_map(|p| p.as_tool_response())
            .collect()
    }

    /// Consumes the `Content` and returns all `ToolResponse` parts.
    #[must_use] pub fn into_tool_responses(self) -> Vec<ToolResponse> {
        self.parts
            .into_iter()
            .filter_map(super::content_part::ContentPart::into_tool_response)
            .collect()
    }

    /// Returns the first text part, if any.
    #[must_use] pub fn first_text(&self) -> Option<&str> {
        self.parts.iter().find_map(|p| p.as_text())
    }

    /// Consumes the `Content` and returns the first text part as a `String`.
    #[must_use] pub fn into_first_text(self) -> Option<String> {
        self.parts.into_iter().find_map(super::content_part::ContentPart::into_text)
    }

    /// Joins all text parts into a single `String`.
    #[must_use] pub fn joined_texts(&self) -> Option<String> {
        let texts = self.texts();
        if texts.is_empty() {
            return None;
        }

        if texts.len() == 1 {
            return texts.first().map(|s| (*s).to_string());
        }

        let mut combined = String::new();
        for text in texts {
            append_text(&mut combined, text);
        }
        Some(combined)
    }

    /// Consumes the `Content` and joins all text parts into a single `String`.
    #[must_use] pub fn into_joined_texts(self) -> Option<String> {
        let texts = self.into_texts();
        if texts.is_empty() {
            return None;
        }

        if texts.len() == 1 {
            return texts.into_iter().next();
        }

        let mut combined = String::new();
        for text in texts {
            append_text(&mut combined, &text);
        }
        Some(combined)
    }

    /// Returns `true` if the content has no parts.
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Returns the number of parts in the content.
    #[must_use] pub const fn len(&self) -> usize {
        self.parts.len()
    }

    /// Returns `true` if all parts are empty or whitespace-only text.
    #[must_use] pub fn is_text_empty(&self) -> bool {
        if self.parts.is_empty() {
            return true;
        }
        self.parts
            .iter()
            .all(|p| matches!(p, ContentPart::Text(t) if t.trim().is_empty()))
    }

    /// Returns `true` if all parts are text.
    #[must_use] pub fn is_text_only(&self) -> bool {
        self.parts.iter().all(|p| p.as_text().is_some())
    }

    /// Returns `true` if there is at least one text part.
    #[must_use] pub fn has_text(&self) -> bool {
        self.parts.iter().any(|p| p.as_text().is_some())
    }

    /// Returns `true` if there is at least one `ToolCall` part.
    #[must_use] pub fn has_tool_calls(&self) -> bool {
        self.parts.iter().any(|p| p.as_tool_call().is_some())
    }

    /// Returns `true` if there is at least one `ToolResponse` part.
    #[must_use] pub fn has_tool_responses(&self) -> bool {
        self.parts.iter().any(|p| p.as_tool_response().is_some())
    }
}

impl Extend<ContentPart> for Content {
    fn extend<T: IntoIterator<Item = ContentPart>>(&mut self, iter: T) {
        self.parts.extend(iter);
    }
}

impl IntoIterator for Content {
    type Item = ContentPart;
    type IntoIter = std::vec::IntoIter<ContentPart>;
    fn into_iter(self) -> Self::IntoIter {
        self.parts.into_iter()
    }
}

impl<'a> IntoIterator for &'a Content {
    type Item = &'a ContentPart;
    type IntoIter = Iter<'a, ContentPart>;
    fn into_iter(self) -> Self::IntoIter {
        self.parts.iter()
    }
}

impl<'a> IntoIterator for &'a mut Content {
    type Item = &'a mut ContentPart;
    type IntoIter = IterMut<'a, ContentPart>;
    fn into_iter(self) -> Self::IntoIter {
        self.parts.iter_mut()
    }
}

impl FromIterator<ContentPart> for Content {
    fn from_iter<T: IntoIterator<Item = ContentPart>>(iter: T) -> Self {
        Self {
            parts: iter.into_iter().collect(),
        }
    }
}

impl From<&str> for Content {
    fn from(s: &str) -> Self {
        Self {
            parts: vec![ContentPart::Text(s.to_string())],
        }
    }
}

impl From<&String> for Content {
    fn from(s: &String) -> Self {
        Self {
            parts: vec![ContentPart::Text(s.to_string())],
        }
    }
}

impl From<String> for Content {
    fn from(s: String) -> Self {
        Self {
            parts: vec![ContentPart::Text(s)],
        }
    }
}

impl From<Vec<ToolCall>> for Content {
    fn from(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            parts: tool_calls.into_iter().map(ContentPart::ToolCall).collect(),
        }
    }
}

impl From<ToolResponse> for Content {
    fn from(tool_response: ToolResponse) -> Self {
        Self {
            parts: vec![ContentPart::ToolResponse(tool_response)],
        }
    }
}

impl From<Vec<ContentPart>> for Content {
    fn from(parts: Vec<ContentPart>) -> Self {
        Self { parts }
    }
}

impl From<ContentPart> for Content {
    fn from(part: ContentPart) -> Self {
        Self { parts: vec![part] }
    }
}

fn append_text(combined: &mut String, text: &str) {
    if !combined.is_empty() {
        combined.push_str("\n\n");
    }
    combined.push_str(text);
}

// ============================================================================
// A2A Conversions
// ============================================================================

impl From<a2a_types::Message> for Content {
    fn from(msg: a2a_types::Message) -> Self {
        let parts: Vec<ContentPart> = msg.parts.into_iter().map(ContentPart::from).collect();
        Self::from_parts(parts)
    }
}
