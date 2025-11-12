//! Agent builder and definition types.
//!
//! This module provides a fluent builder API for constructing agent definitions
//! that can be deployed to the runtime. Agents are composed of metadata (id, name,
//! description) and registered skills.
//!
//! # Overview
//!
//! - [`Agent`]: Entry point for the builder API
//! - [`AgentBuilder`]: Fluent builder for agent definitions
//! - [`AgentDefinition`]: Complete agent specification
//! - [`SkillRegistration`]: Internal representation of registered skills
//!
//! # Examples
//!
//! ```ignore
//! use radkit::agent::Agent;
//!
//! let agent = Agent::builder()
//!     .with_id("weather-agent")
//!     .with_name("Weather Assistant")
//!     .with_description("Provides weather information")
//!     .with_skill(MyForecastSkill)
//!     .build();
//! ```

use crate::agent::skill::{RegisteredSkill, SkillHandler, SkillMetadata};
use std::sync::Arc;
use uuid::Uuid;

const DEFAULT_AGENT_VERSION: &str = "0.0.1";

/// Declarative definition for an agent that can be deployed to the runtime.
///
/// An agent definition contains all the metadata and registered skills needed
/// to deploy an agent to the runtime. Use [`Agent::builder()`] to construct
/// agent definitions.
pub struct AgentDefinition {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) description: Option<String>,
    pub(crate) dispatcher_prompt: Option<String>,
    pub(crate) skills: Vec<SkillRegistration>,
}

/// Fluent builder for constructing [`AgentDefinition`] instances.
///
/// Provides a chainable API for setting agent properties and registering skills.
/// Obtain a builder through [`Agent::builder()`].
pub struct AgentBuilder {
    inner: AgentDefinition,
}

/// Marker struct used to qualify shared builder entry point methods.
///
/// This type provides the static entry point [`Agent::builder()`] for
/// constructing new agents.
pub struct Agent;

/// Internal representation of a skill registered against an agent.
///
/// Contains the skill metadata and handler implementation. This type is
/// used internally by the agent definition.
pub struct SkillRegistration {
    pub(crate) metadata: &'static SkillMetadata,
    pub(crate) handler: Arc<dyn SkillHandler>,
}

impl Agent {
    /// Creates a new agent builder.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let agent = Agent::builder()
    ///     .with_id("my-agent")
    ///     .with_name("My Agent")
    ///     .build();
    /// ```
    #[must_use]
    pub fn builder() -> AgentBuilder {
        AgentBuilder {
            inner: AgentDefinition {
                id: String::new(),
                name: String::new(),
                version: DEFAULT_AGENT_VERSION.to_string(),
                description: None,
                dispatcher_prompt: None,
                skills: Vec::new(),
            },
        }
    }
}

impl AgentBuilder {
    /// Sets the agent identifier.
    ///
    /// The ID uniquely identifies this agent within the runtime.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the agent
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = Agent::builder()
    ///     .with_id("weather-agent");
    /// ```
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.inner.id = id.into();
        self
    }

    /// Sets the agent display name.
    ///
    /// The name is used for display purposes and logging.
    ///
    /// # Arguments
    ///
    /// * `name` - Human-readable name for the agent
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = Agent::builder()
    ///     .with_name("Weather Assistant");
    /// ```
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.inner.name = name.into();
        self
    }

    /// Sets the agent version string used for versioned transport routes.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.inner.version = version.into();
        self
    }

    /// Sets the agent description.
    ///
    /// The description provides context about the agent's purpose and capabilities.
    ///
    /// # Arguments
    ///
    /// * `description` - Human-readable description
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = Agent::builder()
    ///     .with_description("Provides weather forecasts and alerts");
    /// ```
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.inner.description = Some(description.into());
        self
    }

    /// Sets the dispatcher prompt used by the runtime.
    ///
    /// The dispatcher prompt guides the runtime's decision-making when
    /// selecting skills to invoke.
    ///
    /// # Arguments
    ///
    /// * `prompt` - Dispatcher prompt text
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = Agent::builder()
    ///     .with_dispatcher_prompt("Select the appropriate weather skill");
    /// ```
    pub fn with_dispatcher_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.inner.dispatcher_prompt = Some(prompt.into());
        self
    }

    /// Registers a skill with the agent.
    ///
    /// The skill must implement [`RegisteredSkill`], typically derived by the
    /// `#[skill]` macro. Metadata such as the id, name, and MIME modes are sourced
    /// from the skill implementation itself.
    pub fn with_skill<T>(mut self, skill: T) -> Self
    where
        T: RegisteredSkill + 'static,
    {
        let metadata = T::metadata();
        self.inner.skills.push(SkillRegistration {
            metadata,
            handler: Arc::new(skill),
        });
        self
    }

    /// Finalizes and returns the agent definition.
    ///
    /// Consumes the builder and returns a complete [`AgentDefinition`]
    /// ready for deployment to the runtime.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let agent = Agent::builder()
    ///     .with_id("my-agent")
    ///     .with_name("My Agent")
    ///     .build();
    /// ```
    #[must_use]
    pub fn build(mut self) -> AgentDefinition {
        if self.inner.id.is_empty() {
            self.inner.id = Uuid::new_v4().to_string();
        }
        if self.inner.version.trim().is_empty() {
            self.inner.version = DEFAULT_AGENT_VERSION.to_string();
        }
        self.inner
    }
}

impl AgentDefinition {
    /// Returns the agent identifier.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// println!("Agent ID: {}", agent.id());
    /// ```
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the agent name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the agent version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the agent description, if set.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the dispatcher prompt, if set.
    #[must_use]
    pub fn dispatcher_prompt(&self) -> Option<&str> {
        self.dispatcher_prompt.as_deref()
    }

    /// Returns a slice of registered skills.
    #[must_use]
    pub fn skills(&self) -> &[SkillRegistration] {
        &self.skills
    }
}

impl SkillRegistration {
    /// Returns the skill name.
    #[must_use]
    pub const fn name(&self) -> &str {
        self.metadata.name
    }

    /// Returns the skill identifier.
    #[must_use]
    pub const fn id(&self) -> &str {
        self.metadata.id
    }

    /// Returns the skill description.
    #[must_use]
    pub const fn description(&self) -> &str {
        self.metadata.description
    }

    /// Returns a reference to the skill handler.
    #[must_use]
    pub fn handler(&self) -> &dyn SkillHandler {
        &*self.handler
    }

    /// Returns a shareable handle to the skill handler.
    #[must_use]
    pub fn handler_arc(&self) -> Arc<dyn SkillHandler> {
        Arc::clone(&self.handler)
    }

    /// Returns the metadata associated with the skill.
    #[must_use]
    pub const fn metadata(&self) -> &'static SkillMetadata {
        self.metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::skill::{OnInputResult, OnRequestResult, SkillSlot};
    use crate::errors::AgentError;
    use crate::models::Content;

    #[test]
    fn build_generates_id_and_applies_defaults() {
        let agent = Agent::builder().with_name("Test").build();

        assert!(!agent.id().is_empty(), "expected builder to generate an id");
        assert_eq!(agent.name(), "Test");
        assert_eq!(agent.version(), DEFAULT_AGENT_VERSION);
        assert!(agent.description().is_none());
        assert!(agent.dispatcher_prompt().is_none());
        assert!(agent.skills().is_empty());
    }

    #[test]
    fn build_preserves_all_fields() {
        let agent = Agent::builder()
            .with_id("custom-id")
            .with_name("Custom Agent")
            .with_version("1.2.3")
            .with_description("A helpful description")
            .with_dispatcher_prompt("Route wisely")
            .with_skill(DummySkill)
            .build();

        assert_eq!(agent.id(), "custom-id");
        assert_eq!(agent.name(), "Custom Agent");
        assert_eq!(agent.version(), "1.2.3");
        assert_eq!(agent.description(), Some("A helpful description"));
        assert_eq!(agent.dispatcher_prompt(), Some("Route wisely"));

        let skills = agent.skills();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id(), DummySkill::metadata().id);
        assert_eq!(skills[0].name(), DummySkill::metadata().name);
    }

    #[test]
    fn skills_maintain_registration_order() {
        let agent = Agent::builder()
            .with_skill(DummySkill)
            .with_skill(SecondarySkill)
            .build();

        let skills = agent.skills();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].id(), DummySkill::metadata().id);
        assert_eq!(skills[1].id(), SecondarySkill::metadata().id);
    }

    struct DummySkill;

    static DUMMY_METADATA: SkillMetadata = SkillMetadata::new(
        "dummy_skill",
        "Dummy Skill",
        "A test skill used for verifying registration behaviour.",
        &[],
        &[],
        &[],
        &[],
    );

    #[cfg_attr(
        all(target_os = "wasi", target_env = "p1"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(target_os = "wasi", target_env = "p1")),
        async_trait::async_trait
    )]
    impl SkillHandler for DummySkill {
        async fn on_request(
            &self,
            _task_context: &mut crate::runtime::context::TaskContext,
            _context: &crate::runtime::context::Context,
            _runtime: &dyn crate::runtime::Runtime,
            _content: Content,
        ) -> Result<OnRequestResult, AgentError> {
            Ok(OnRequestResult::Completed {
                message: None,
                artifacts: Vec::new(),
            })
        }

        async fn on_input_received(
            &self,
            _task_context: &mut crate::runtime::context::TaskContext,
            _context: &crate::runtime::context::Context,
            _runtime: &dyn crate::runtime::Runtime,
            _content: Content,
        ) -> Result<OnInputResult, AgentError> {
            Ok(OnInputResult::InputRequired {
                message: Content::from_text("Need more input"),
                slot: SkillSlot::new("dummy"),
            })
        }
    }

    impl RegisteredSkill for DummySkill {
        fn metadata() -> &'static SkillMetadata {
            &DUMMY_METADATA
        }
    }

    struct SecondarySkill;

    static SECONDARY_METADATA: SkillMetadata = SkillMetadata::new(
        "secondary_skill",
        "Secondary Skill",
        "A second skill for ordering tests.",
        &[],
        &[],
        &[],
        &[],
    );

    #[cfg_attr(
        all(target_os = "wasi", target_env = "p1"),
        async_trait::async_trait(?Send)
    )]
    #[cfg_attr(
        not(all(target_os = "wasi", target_env = "p1")),
        async_trait::async_trait
    )]
    impl SkillHandler for SecondarySkill {
        async fn on_request(
            &self,
            _task_context: &mut crate::runtime::context::TaskContext,
            _context: &crate::runtime::context::Context,
            _runtime: &dyn crate::runtime::Runtime,
            _content: Content,
        ) -> Result<OnRequestResult, AgentError> {
            Ok(OnRequestResult::Rejected {
                reason: Content::from_text("Not supported"),
            })
        }
    }

    impl RegisteredSkill for SecondarySkill {
        fn metadata() -> &'static SkillMetadata {
            &SECONDARY_METADATA
        }
    }
}
