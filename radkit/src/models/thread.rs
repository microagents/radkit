//! Conversation threads for LLM interactions.
//!
//! This module provides the [`Thread`] type, which represents a complete conversation
//! thread including an optional system prompt and a sequence of events (messages).
//! Threads are the primary way to structure multi-turn conversations with LLMs.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::models::{Thread, Event};
//!
//! // Create a simple thread from a user message
//! let thread = Thread::from_user("What is 2+2?");
//!
//! // Create a thread with a system prompt
//! let thread = Thread::from_system("You are a math tutor")
//!     .add_event(Event::user("What is 2+2?"));
//!
//! // Build a complex thread
//! let thread = Thread::new(vec![
//!     Event::user("Hello"),
//!     Event::assistant("Hi! How can I help?"),
//!     Event::user("Tell me about Rust"),
//! ]);
//! ```

use crate::models::content::Content;
use crate::models::content_part::ContentPart;
use crate::models::event::Event;
use serde::{Deserialize, Serialize};

/// A conversation thread containing a system prompt and a sequence of events.
///
/// Threads represent the full context of a conversation with an LLM, including
/// an optional system prompt that provides instructions or context, and a series
/// of events representing the back-and-forth messages.
///
/// Use the builder pattern with methods like [`with_system`](Thread::with_system),
/// [`add_event`](Thread::add_event), and [`add_events`](Thread::add_events) to
/// construct complex threads.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Thread {
    system: Option<String>,
    #[serde(default)]
    events: Vec<Event>,
}

impl Thread {
    /// Creates a new `Thread` from a vector of `Event`s.
    #[must_use]
    pub const fn new(events: Vec<Event>) -> Self {
        Self {
            events,
            system: None,
        }
    }

    /// Creates a new `Thread` with an initial system prompt.
    pub fn from_system(content: impl Into<String>) -> Self {
        Self {
            system: Some(content.into()),
            events: Vec::new(),
        }
    }

    /// Creates a new `Thread` with a single user event.
    pub fn from_user(content: impl Into<String>) -> Self {
        Self {
            system: None,
            events: vec![Event::user(content.into())],
        }
    }

    /// Sets or replaces the system prompt.
    #[must_use]
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Adds a single event to the thread.
    #[must_use]
    pub fn add_event(mut self, event: impl Into<Event>) -> Self {
        self.events.push(event.into());
        self
    }

    /// Adds multiple events to the thread.
    #[must_use]
    pub fn add_events<I>(mut self, events: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Event>,
    {
        self.events.extend(events.into_iter().map(Into::into));
        self
    }

    /// Returns a reference to the system prompt, if any.
    #[must_use]
    pub fn system(&self) -> Option<&str> {
        self.system.as_deref()
    }

    /// Returns a reference to the events in this thread.
    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Consumes the thread and returns the events.
    #[must_use]
    pub fn into_events(self) -> Vec<Event> {
        self.events
    }

    /// Consumes the thread and returns both system prompt and events.
    #[must_use]
    pub fn into_parts(self) -> (Option<String>, Vec<Event>) {
        (self.system, self.events)
    }

    /// Converts the thread into a prompt string.
    #[must_use]
    pub fn to_prompt(&self) -> String {
        let mut prompt = String::new();

        if let Some(system) = &self.system {
            use std::fmt::Write;
            let _ = write!(prompt, "<system>\n{system}\n</system>\n\n");
        }

        let event_prompts: Vec<String> = self
            .events
            .iter()
            .map(|event| {
                let content = event.content().joined_texts().unwrap_or_default();
                format!("<{}>\n{}\n</{}>", event.role(), content, event.role())
            })
            .collect();

        prompt.push_str(&event_prompts.join("\n\n"));

        prompt
    }
}

impl From<Content> for Thread {
    /// Treats the content as a user message.
    fn from(content: Content) -> Self {
        Self::new(vec![Event::user(content)])
    }
}

impl From<ContentPart> for Thread {
    /// Treats the content part as a user message.
    fn from(part: ContentPart) -> Self {
        Self::from(Content::from(part))
    }
}

impl From<Event> for Thread {
    /// Creates a `Thread` with a single event.
    fn from(event: Event) -> Self {
        Self::new(vec![event])
    }
}

impl From<Vec<Event>> for Thread {
    /// Creates a `Thread` with the given events.
    fn from(events: Vec<Event>) -> Self {
        Self::new(events)
    }
}

impl From<&str> for Thread {
    /// Treats the string as a user message.
    fn from(user: &str) -> Self {
        Self::from_user(user)
    }
}

impl From<&String> for Thread {
    /// Treats the string as a user message.
    fn from(user: &String) -> Self {
        Self::from_user(user)
    }
}

impl From<String> for Thread {
    /// Treats the string as a user message.
    fn from(user: String) -> Self {
        Self::from_user(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Content, Event, Role};

    #[test]
    fn system_and_event_helpers_work() {
        let thread = Thread::from_system("Be concise")
            .add_event(Event::user("Hi"))
            .add_event(Event::assistant("Hello"));

        assert_eq!(thread.system(), Some("Be concise"));
        assert_eq!(thread.events().len(), 2);
        assert!(matches!(thread.events()[0].role(), Role::User));
        assert!(thread.to_prompt().contains("<system>"));
        assert!(thread.to_prompt().contains("<User>"));
    }

    #[test]
    fn conversions_create_expected_threads() {
        let from_content = Thread::from(Content::from_text("Hello"));
        assert_eq!(from_content.events().len(), 1);
        assert!(matches!(from_content.events()[0].role(), Role::User));

        let from_event = Thread::from(Event::assistant("Ready"));
        assert!(matches!(from_event.events()[0].role(), Role::Assistant));

        let from_str = Thread::from("Hi there");
        assert!(matches!(from_str.events()[0].role(), Role::User));
    }
}
