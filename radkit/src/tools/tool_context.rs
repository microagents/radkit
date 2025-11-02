//! Tool execution context.
//!
//! This module provides [`ToolContext`], which carries execution-scoped state
//! and configuration to tools during invocation.
//!
//! # Overview
//!
//! - [`ToolContext`]: Execution context with access to state
//! - [`ToolContextBuilder`]: Builder for constructing contexts
//!
//! # Examples
//!
//! ```ignore
//! use radkit::tools::{ToolContext, DefaultExecutionState};
//!
//! let state = DefaultExecutionState::new();
//! let context = ToolContext::builder()
//!     .with_state(&state)
//!     .build()?;
//!
//! // Use context in tool execution
//! context.state().set_state("key", json!("value"));
//! ```

use super::execution_state::ExecutionState;
use crate::errors::{AgentError, AgentResult};
use std::fmt;

/// Context passed to tools during execution.
///
/// Provides access to execution state and potentially other contextual
/// information needed by tools. The context is immutable and borrowed
/// for the lifetime of tool execution.
#[derive(Clone)]
pub struct ToolContext<'a> {
    state: &'a dyn ExecutionState,
}

impl<'a> ToolContext<'a> {
    #[must_use]
    pub fn builder() -> ToolContextBuilder<'a> {
        ToolContextBuilder::default()
    }

    pub fn new(state: &'a dyn ExecutionState) -> Self {
        Self { state }
    }

    #[must_use]
    pub fn state(&self) -> &dyn ExecutionState {
        self.state
    }
}

#[derive(Default)]
pub struct ToolContextBuilder<'a> {
    state: Option<&'a dyn ExecutionState>,
}

impl<'a> ToolContextBuilder<'a> {
    pub fn with_state(mut self, state: &'a dyn ExecutionState) -> Self {
        self.state = Some(state);
        self
    }

    /// Builds the [`ToolContext`] from the builder.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Validation`] if `with_state()` was not called.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::tools::{ToolContext, DefaultExecutionState};
    ///
    /// let state = DefaultExecutionState::new();
    /// let context = ToolContext::builder()
    ///     .with_state(&state)
    ///     .build()?;
    /// ```
    pub fn build(self) -> AgentResult<ToolContext<'a>> {
        let state = self.state.ok_or(AgentError::Validation {
            field: "ToolContextBuilder.state".to_string(),
            reason: "Execution state is required, but was not provided. Call `.with_state()` on the builder.".to_string(),
        })?;
        Ok(ToolContext { state })
    }
}

impl fmt::Debug for ToolContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ToolContext");
        debug.field("state", &"<ExecutionState>");
        debug.finish()
    }
}

impl fmt::Debug for ToolContextBuilder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ToolContextBuilder");
        debug.field(
            "state",
            &self
                .state
                .as_ref()
                .map_or("<unset>", |_| "<ExecutionState>"),
        );
        debug.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::execution_state::DefaultExecutionState;

    #[test]
    fn builder_requires_state() {
        let error = ToolContext::builder().build().expect_err("missing state");
        match error {
            AgentError::Validation { field, .. } => {
                assert_eq!(field, "ToolContextBuilder.state");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn builder_provides_state_reference() {
        let state = DefaultExecutionState::new();
        let ctx = ToolContext::builder()
            .with_state(&state)
            .build()
            .expect("context");

        ctx.state()
            .set_state("user", serde_json::Value::from("alice"));
        assert_eq!(
            ctx.state().get_state("user"),
            Some(serde_json::Value::from("alice"))
        );
    }
}
