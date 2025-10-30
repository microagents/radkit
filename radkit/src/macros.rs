//! Stub macros for radkit agents.
//!
//! This module provides stub attribute macros that allow code to compile.
//! Full procedural macro implementations will be added later.

// Stub macros that do nothing - just for compilation

/// Stub attribute macro for defining A2A skills.
///
/// Currently does nothing - the examples won't compile yet but the types are defined.
///
/// TODO: Implement as proc macro to generate A2A metadata
#[allow(unused_imports)]
pub use stub_skill as skill;

/// Stub attribute macro for marking entrypoints.
///
/// Currently does nothing - the examples won't compile yet but the types are defined.
///
/// TODO: Implement as proc macro to generate WASM exports
#[allow(unused_imports)]
pub use stub_entrypoint as entrypoint;

// These will be replaced with real proc macros later
#[doc(hidden)]
pub const fn stub_skill() {}

#[doc(hidden)]
pub const fn stub_entrypoint() {}
