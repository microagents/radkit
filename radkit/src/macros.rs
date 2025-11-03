//! Macros for radkit agents.
//!
//! This module provides procedural macros for defining agent skills and entrypoints.

// Re-export the skill macro from radkit-macros crate

/// Stub attribute macro for marking entrypoints.
///
/// Currently does nothing - the examples won't compile yet but the types are defined.
///
/// TODO: Implement as proc macro to generate WASM exports
#[allow(unused_imports)]
pub use stub_entrypoint as entrypoint;

// This will be replaced with real proc macro later
#[doc(hidden)]
pub const fn stub_entrypoint() {}
