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
    pub(crate) handler: Box<dyn SkillHandler>,
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
    #[must_use] pub fn builder() -> AgentBuilder {
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
            handler: Box::new(skill),
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
    #[must_use] pub fn build(mut self) -> AgentDefinition {
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
    #[must_use] pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the agent name.
    #[must_use] pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the agent version string.
    #[must_use] pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the agent description, if set.
    #[must_use] pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the dispatcher prompt, if set.
    #[must_use] pub fn dispatcher_prompt(&self) -> Option<&str> {
        self.dispatcher_prompt.as_deref()
    }

    /// Returns a slice of registered skills.
    #[must_use] pub fn skills(&self) -> &[SkillRegistration] {
        &self.skills
    }
}

impl SkillRegistration {
    /// Returns the skill name.
    #[must_use] pub const fn name(&self) -> &str {
        self.metadata.name
    }

    /// Returns the skill identifier.
    #[must_use] pub const fn id(&self) -> &str {
        self.metadata.id
    }

    /// Returns the skill description.
    #[must_use] pub const fn description(&self) -> &str {
        self.metadata.description
    }

    /// Returns a reference to the skill handler.
    #[must_use] pub fn handler(&self) -> &dyn SkillHandler {
        &*self.handler
    }

    /// Returns the metadata associated with the skill.
    #[must_use] pub const fn metadata(&self) -> &'static SkillMetadata {
        self.metadata
    }
}
