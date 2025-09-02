use crate::a2a::{SendMessageResult, SendStreamingMessageResult};
use crate::sessions::SessionEvent;
use futures::Stream;
use std::pin::Pin;

/// Result from send_message that includes captured events
pub struct SendMessageResultWithEvents {
    /// The actual result (Task or Message)
    pub result: SendMessageResult,
    /// All events emitted during execution (including non-A2A events)
    pub all_events: Vec<SessionEvent>,
    /// A2A-compatible events only
    pub a2a_events: Vec<SendStreamingMessageResult>,
}

/// Result from send_streaming_message with both streams
pub struct SendStreamingMessageResultWithEvents {
    /// Stream of A2A-compatible events only
    pub a2a_stream: Pin<Box<dyn Stream<Item = SendStreamingMessageResult> + Send>>,
    /// Stream of all events (including non-A2A events)
    pub all_events_stream: Pin<Box<dyn Stream<Item = SessionEvent> + Send>>,
}
