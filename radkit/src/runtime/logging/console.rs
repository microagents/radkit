//! Console logging service implementation.

use crate::runtime::logging::{LogLevel, LoggingService};

/// Console logging service that prints to stderr.
///
/// This implementation prints log messages to stderr with a simple format
/// that includes the log level and message. It's suitable for local development
/// and testing.
///
/// # Examples
///
/// ```
/// use radkit::runtime::logging::{LoggingService, LogLevel, ConsoleLoggingService};
///
/// let logger = ConsoleLoggingService;
/// logger.log(LogLevel::Info, "Agent started");
/// logger.log(LogLevel::Error, "Failed to process request");
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ConsoleLoggingService;

impl LoggingService for ConsoleLoggingService {
    fn log(&self, level: LogLevel, message: &str) {
        let level_str = match level {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        };

        eprintln!("[{level_str}] {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_logging_formats_level() {
        let logger = ConsoleLoggingService;
        logger.log(LogLevel::Info, "console");
    }
}
