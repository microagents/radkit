//! Agent definition primitives.

pub mod builder;
pub mod llm_function;
pub mod llm_worker;
pub mod skill;
pub mod structured_parser;

pub use builder::{Agent, AgentBuilder, AgentDefinition, SkillRegistration};
pub use llm_function::LlmFunction;
pub use llm_worker::{LlmWorker, LlmWorkerBuilder};
pub use skill::{
    Artifact, OnInputResult, OnRequestResult, RegisteredSkill, SkillHandler, SkillMetadata,
    SkillSlot,
};
