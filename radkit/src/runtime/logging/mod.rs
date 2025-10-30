//! Logging service for structured logging.
//!
//! This module provides logging functionality for agents and runtime components.
//! The [`LoggingService`] trait defines the interface for emitting log messages
//! with different severity levels.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::runtime::logging::{LoggingService, LogLevel, ConsoleLoggingService};
//!
//! let logger = ConsoleLoggingService;
//! logger.log(LogLevel::Info, "Starting agent execution");
//! logger.log(LogLevel::Error, "Failed to process request");
//! ```

pub mod console;

pub use console::ConsoleLoggingService;

use crate::compat::{MaybeSend, MaybeSync};

/// Service for structured logging.
///
/// Logging service allows agents and runtimes to emit log messages
/// with different severity levels.
pub trait LoggingService: MaybeSend + MaybeSync {
    /// Logs a message at the specified level.
    ///
    /// The default implementation is a no-op. Implementations should
    /// override this to provide actual logging.
    ///
    /// # Arguments
    ///
    /// * `level` - Log severity level
    /// * `message` - Message to log
    fn log(&self, _level: LogLevel, _message: &str) {
        // Default no-op implementation
    }
}

/// Log severity levels.
///
/// This enum is marked `#[non_exhaustive]` to allow adding new levels
/// in future versions without breaking existing code.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Trace-level logging (most verbose).
    Trace,
    /// Debug-level logging.
    Debug,
    /// Informational logging.
    Info,
    /// Warn-level logging.
    Warn,
    /// Error-level logging (least verbose).
    Error,
}
