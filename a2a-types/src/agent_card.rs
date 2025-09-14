use crate::SecurityScheme;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
// ============================================================================
// A2A Agent Card and Discovery Types
// ============================================================================

/// Supported A2A transport protocols.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransportProtocol {
    /// JSON-RPC 2.0 over HTTP
    #[serde(rename = "JSONRPC")]
    JsonRpc,
    /// gRPC over HTTP/2
    #[serde(rename = "GRPC")]
    Grpc,
    /// REST-style HTTP with JSON
    #[serde(rename = "HTTP+JSON")]
    HttpJson,
}

impl Default for TransportProtocol {
    fn default() -> Self {
        TransportProtocol::JsonRpc
    }
}

/// Declares a combination of a target URL and a transport protocol for interacting with the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInterface {
    /// The transport protocol supported at this URL.
    pub transport: TransportProtocol,
    /// The URL where this interface is available.
    pub url: String,
}

/// A declaration of a protocol extension supported by an Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExtension {
    /// The unique URI identifying the extension.
    pub uri: String,
    /// A human-readable description of how this agent uses the extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// If true, the client must understand and comply with the extension's requirements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Optional, extension-specific configuration parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

/// Defines optional capabilities supported by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    /// Indicates if the agent supports Server-Sent Events (SSE) for streaming responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    /// Indicates if the agent supports sending push notifications for asynchronous task updates.
    #[serde(skip_serializing_if = "Option::is_none", rename = "pushNotifications")]
    pub push_notifications: Option<bool>,
    /// Indicates if the agent provides a history of state transitions for a task.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "stateTransitionHistory"
    )]
    pub state_transition_history: Option<bool>,
    /// A list of protocol extensions supported by the agent.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extensions: Vec<AgentExtension>,
}

/// Represents the service provider of an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProvider {
    /// The name of the agent provider's organization.
    pub organization: String,
    /// A URL for the agent provider's website or relevant documentation.
    pub url: String,
}

/// Represents a distinct capability or function that an agent can perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    /// A unique identifier for the agent's skill.
    pub id: String,
    /// A human-readable name for the skill.
    pub name: String,
    /// A detailed description of the skill.
    pub description: String,
    /// A set of keywords describing the skill's capabilities.
    pub tags: Vec<String>,
    /// Example prompts or scenarios that this skill can handle.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub examples: Vec<String>,
    /// The set of supported input MIME types for this skill, overriding the agent's defaults.
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "inputModes", default)]
    pub input_modes: Vec<String>,
    /// The set of supported output MIME types for this skill, overriding the agent's defaults.
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "outputModes", default)]
    pub output_modes: Vec<String>,
    /// Security schemes necessary for the agent to leverage this skill.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub security: Vec<HashMap<String, Vec<String>>>,
}

/// Represents a JWS signature of an AgentCard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardSignature {
    /// The protected JWS header for the signature (Base64url-encoded).
    #[serde(rename = "protected")]
    pub protected_header: String,
    /// The computed signature (Base64url-encoded).
    pub signature: String,
    /// The unprotected JWS header values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, serde_json::Value>>,
}

/// The AgentCard is a self-describing manifest for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// A human-readable name for the agent.
    pub name: String,
    /// A human-readable description of the agent.
    pub description: String,
    /// The agent's own version number.
    pub version: String,
    /// The version of the A2A protocol this agent supports.
    #[serde(rename = "protocolVersion", default = "default_protocol_version")]
    pub protocol_version: String,
    /// The preferred endpoint URL for interacting with the agent.
    pub url: String,
    /// The transport protocol for the preferred endpoint.
    #[serde(rename = "preferredTransport", default)]
    pub preferred_transport: TransportProtocol,
    /// A declaration of optional capabilities supported by the agent.
    pub capabilities: AgentCapabilities,
    /// Default set of supported input MIME types for all skills.
    #[serde(rename = "defaultInputModes")]
    pub default_input_modes: Vec<String>,
    /// Default set of supported output MIME types for all skills.
    #[serde(rename = "defaultOutputModes")]
    pub default_output_modes: Vec<String>,
    /// The set of skills that the agent can perform.
    pub skills: Vec<AgentSkill>,
    /// Information about the agent's service provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// A list of additional supported interfaces (transport and URL combinations).
    #[serde(
        skip_serializing_if = "Vec::is_empty",
        rename = "additionalInterfaces",
        default
    )]
    pub additional_interfaces: Vec<AgentInterface>,
    /// An optional URL to the agent's documentation.
    #[serde(skip_serializing_if = "Option::is_none", rename = "documentationUrl")]
    pub documentation_url: Option<String>,
    /// An optional URL to an icon for the agent.
    #[serde(skip_serializing_if = "Option::is_none", rename = "iconUrl")]
    pub icon_url: Option<String>,
    /// A list of security requirement objects that apply to all agent interactions.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub security: Vec<HashMap<String, Vec<String>>>,
    /// A declaration of the security schemes available to authorize requests.
    #[serde(skip_serializing_if = "Option::is_none", rename = "securitySchemes")]
    pub security_schemes: Option<HashMap<String, SecurityScheme>>,
    /// JSON Web Signatures computed for this AgentCard.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub signatures: Vec<AgentCardSignature>,
    /// If true, the agent can provide an extended agent card to authenticated users.
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "supportsAuthenticatedExtendedCard"
    )]
    pub supports_authenticated_extended_card: Option<bool>,
}

fn default_protocol_version() -> String {
    crate::PROTOCOL_VERSION.to_string()
}

impl AgentCard {
    /// Create a new AgentCard with minimal required fields
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        version: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            version: version.into(),
            protocol_version: default_protocol_version(),
            url: url.into(),
            preferred_transport: TransportProtocol::default(),
            capabilities: AgentCapabilities::default(),
            default_input_modes: vec!["text/plain".to_string()],
            default_output_modes: vec!["text/plain".to_string()],
            skills: Vec::new(),
            provider: None,
            additional_interfaces: Vec::new(),
            documentation_url: None,
            icon_url: None,
            security: Vec::new(),
            security_schemes: None,
            signatures: Vec::new(),
            supports_authenticated_extended_card: None,
        }
    }

    /// Set the agent's name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the agent's description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the agent's version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the A2A protocol version
    pub fn with_protocol_version(mut self, protocol_version: impl Into<String>) -> Self {
        self.protocol_version = protocol_version.into();
        self
    }

    /// Set the agent's URL endpoint
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Set the preferred transport protocol
    pub fn with_preferred_transport(mut self, transport: TransportProtocol) -> Self {
        self.preferred_transport = transport;
        self
    }

    /// Set the agent's capabilities
    pub fn with_capabilities(mut self, capabilities: AgentCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Enable streaming capability
    pub fn with_streaming(mut self, enabled: bool) -> Self {
        self.capabilities.streaming = Some(enabled);
        self
    }

    /// Enable push notifications capability
    pub fn with_push_notifications(mut self, enabled: bool) -> Self {
        self.capabilities.push_notifications = Some(enabled);
        self
    }

    /// Enable state transition history capability
    pub fn with_state_transition_history(mut self, enabled: bool) -> Self {
        self.capabilities.state_transition_history = Some(enabled);
        self
    }

    /// Add an extension to the capabilities
    pub fn add_extension(mut self, extension: AgentExtension) -> Self {
        self.capabilities.extensions.push(extension);
        self
    }

    /// Set default input modes (replaces existing)
    pub fn with_default_input_modes(mut self, modes: Vec<String>) -> Self {
        self.default_input_modes = modes;
        self
    }

    /// Add a default input mode
    pub fn add_input_mode(mut self, mode: impl Into<String>) -> Self {
        self.default_input_modes.push(mode.into());
        self
    }

    /// Set default output modes (replaces existing)
    pub fn with_default_output_modes(mut self, modes: Vec<String>) -> Self {
        self.default_output_modes = modes;
        self
    }

    /// Add a default output mode
    pub fn add_output_mode(mut self, mode: impl Into<String>) -> Self {
        self.default_output_modes.push(mode.into());
        self
    }

    /// Set skills (replaces existing)
    pub fn with_skills(mut self, skills: Vec<AgentSkill>) -> Self {
        self.skills = skills;
        self
    }

    /// Add a skill
    pub fn add_skill(mut self, skill: AgentSkill) -> Self {
        self.skills.push(skill);
        self
    }

    /// Create a skill using a builder pattern and add it
    pub fn add_skill_with<F>(mut self, id: impl Into<String>, name: impl Into<String>, f: F) -> Self
    where
        F: FnOnce(AgentSkill) -> AgentSkill,
    {
        let skill = AgentSkill::new(id.into(), name.into());
        self.skills.push(f(skill));
        self
    }

    /// Set the provider information
    pub fn with_provider(
        mut self,
        organization: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        self.provider = Some(AgentProvider {
            organization: organization.into(),
            url: url.into(),
        });
        self
    }

    /// Set additional interfaces (replaces existing)
    pub fn with_additional_interfaces(mut self, interfaces: Vec<AgentInterface>) -> Self {
        self.additional_interfaces = interfaces;
        self
    }

    /// Add an additional interface
    pub fn add_interface(mut self, transport: TransportProtocol, url: impl Into<String>) -> Self {
        self.additional_interfaces.push(AgentInterface {
            transport,
            url: url.into(),
        });
        self
    }

    /// Set documentation URL
    pub fn with_documentation_url(mut self, url: impl Into<String>) -> Self {
        self.documentation_url = Some(url.into());
        self
    }

    /// Set icon URL
    pub fn with_icon_url(mut self, url: impl Into<String>) -> Self {
        self.icon_url = Some(url.into());
        self
    }

    /// Set security requirements
    pub fn with_security(mut self, security: Vec<HashMap<String, Vec<String>>>) -> Self {
        self.security = security;
        self
    }

    /// Add a security requirement
    pub fn add_security_requirement(mut self, requirement: HashMap<String, Vec<String>>) -> Self {
        self.security.push(requirement);
        self
    }

    /// Set security schemes
    pub fn with_security_schemes(
        mut self,
        schemes: HashMap<String, crate::SecurityScheme>,
    ) -> Self {
        self.security_schemes = Some(schemes);
        self
    }

    /// Enable authenticated extended card support
    pub fn with_authenticated_extended_card(mut self, supported: bool) -> Self {
        self.supports_authenticated_extended_card = Some(supported);
        self
    }

    /// Set signatures (replaces existing)
    pub fn with_signatures(mut self, signatures: Vec<AgentCardSignature>) -> Self {
        self.signatures = signatures;
        self
    }

    /// Add a signature
    pub fn add_signature(mut self, signature: AgentCardSignature) -> Self {
        self.signatures.push(signature);
        self
    }
}

impl AgentSkill {
    /// Create a new skill with required fields
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            description: String::new(),
            tags: Vec::new(),
            examples: Vec::new(),
            input_modes: Vec::new(),
            output_modes: Vec::new(),
            security: Vec::new(),
        }
    }

    /// Set the skill description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Add a tag
    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set tags (replaces existing)
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Add an example
    pub fn add_example(mut self, example: impl Into<String>) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Set examples (replaces existing)
    pub fn with_examples(mut self, examples: Vec<String>) -> Self {
        self.examples = examples;
        self
    }

    /// Add an input mode
    pub fn add_input_mode(mut self, mode: impl Into<String>) -> Self {
        self.input_modes.push(mode.into());
        self
    }

    /// Set input modes (replaces existing)
    pub fn with_input_modes(mut self, modes: Vec<String>) -> Self {
        self.input_modes = modes;
        self
    }

    /// Add an output mode
    pub fn add_output_mode(mut self, mode: impl Into<String>) -> Self {
        self.output_modes.push(mode.into());
        self
    }

    /// Set output modes (replaces existing)
    pub fn with_output_modes(mut self, modes: Vec<String>) -> Self {
        self.output_modes = modes;
        self
    }

    /// Add a security requirement
    pub fn add_security(mut self, security: HashMap<String, Vec<String>>) -> Self {
        self.security.push(security);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_card_new() {
        let card = AgentCard::new(
            "Test Agent",
            "A test agent",
            "1.0.0",
            "http://localhost:8080",
        );

        assert_eq!(card.name, "Test Agent");
        assert_eq!(card.description, "A test agent");
        assert_eq!(card.version, "1.0.0");
        assert_eq!(card.url, "http://localhost:8080");
    }

    #[test]
    fn test_agent_card_with_capabilities() {
        let card = AgentCard::new(
            "Test Agent",
            "A test agent",
            "1.0.0",
            "http://localhost:8080",
        )
        .with_streaming(true)
        .with_push_notifications(true)
        .with_state_transition_history(false);

        assert_eq!(card.capabilities.streaming, Some(true));
        assert_eq!(card.capabilities.push_notifications, Some(true));
        assert_eq!(card.capabilities.state_transition_history, Some(false));
    }

    #[test]
    fn test_agent_card_with_skill() {
        let card = AgentCard::new(
            "Test Agent",
            "A test agent",
            "1.0.0",
            "http://localhost:8080",
        )
        .add_skill_with("skill1", "Test Skill", |s| {
            s.with_description("A test skill")
                .add_tag("test")
                .add_tag("example")
                .add_example("Example usage")
        });

        assert_eq!(card.skills.len(), 1);
        let skill = &card.skills[0];
        assert_eq!(skill.id, "skill1");
        assert_eq!(skill.name, "Test Skill");
        assert_eq!(skill.description, "A test skill");
        assert_eq!(skill.tags, vec!["test", "example"]);
        assert_eq!(skill.examples, vec!["Example usage"]);
    }

    #[test]
    fn test_agent_card_defaults() {
        let card = AgentCard::new(
            "Test Agent",
            "Test Description",
            "1.0.0",
            "http://localhost:8080",
        );

        assert_eq!(card.default_input_modes, vec!["text/plain"]);
        assert_eq!(card.default_output_modes, vec!["text/plain"]);
    }

    #[test]
    fn test_modify_existing_card() {
        let card = AgentCard::new(
            "Original Agent",
            "Original description",
            "1.0.0",
            "http://localhost:8080",
        )
        .with_name("Modified Agent")
        .with_version("2.0.0")
        .with_streaming(true);

        assert_eq!(card.name, "Modified Agent");
        assert_eq!(card.description, "Original description");
        assert_eq!(card.version, "2.0.0");
        assert_eq!(card.capabilities.streaming, Some(true));
    }
}
