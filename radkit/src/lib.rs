//! # radkit
//!
//! A Rust SDK for building AI agents that run on native and WASM targets.
//!
//! ## Overview
//!
//! radkit provides:
//! - **Agent Runtime**: Execute agents with skills and tool calling
//! - **LLM Integration**: Unified interface for multiple LLM providers
//! - **Tool System**: Extensible tool/function calling framework
//! - **Cross-Platform**: Native (tokio) and WASM (single-threaded) support
//! - **Type Safety**: Strong types with comprehensive error handling
//!
//! ## Quick Start
//!
//! ```ignore
//! use radkit::prelude::*;
//!
//! // Create a simple agent
//! let agent = Agent::builder()
//!     .with_id("my-agent")
//!     .with_name("My Agent")
//!     .build();
//!
//! // Define tools
//! let tool = FunctionTool::new(
//!     "get_weather",
//!     "Get weather for a location",
//!     |args, _ctx| Box::pin(async move {
//!         ToolResult::success(json!({"temp": 72}))
//!     })
//! );
//!
//! // Create a runtime
//! let runtime = DefaultRuntime::new();
//! ```
//!
//! ## Feature Flags
//!
//! radkit uses feature flags to control optional dependencies:
//! - (Features will be documented as they're added)
//!
//! ## Platform Support
//!
//! - **Native** (Linux, macOS, Windows): Full async runtime with tokio
//! - **WASM** (wasm32-wasi, wasm32-unknown-unknown): Single-threaded execution
//!
//! ## Architecture
//!
//! - [`agent`]: Agent definitions and builders
//! - [`tools`]: Tool system for function calling
//! - [`runtime`]: Runtime services and execution
//! - [`errors`]: Typed error handling
//! - [`compat`]: Cross-platform compatibility layer

#![deny(unsafe_code, unreachable_patterns, unused_must_use)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

pub mod agent;
pub mod compat;
pub mod errors;
mod macros;
pub mod models;
pub mod runtime;
pub mod tools;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use crate::compat::{MaybeSend, MaybeSync};
