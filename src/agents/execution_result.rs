use crate::a2a::{SendMessageResult, SendStreamingMessageResult};
use crate::events::InternalEvent;
use futures::Stream;
use std::pin::Pin;
use tokio::sync::mpsc;

/// Result from send_message that includes captured events
pub struct SendMessageResultWithEvents {
    /// The actual result (Task or Message)
    pub result: SendMessageResult,
    /// All internal events emitted during execution
    pub internal_events: Vec<InternalEvent>,
    /// All A2A events emitted during execution
    pub a2a_events: Vec<SendStreamingMessageResult>,
}

/// Result from send_streaming_message that includes internal event stream
pub struct SendStreamingMessageResultWithEvents {
    /// The A2A event stream (unchanged behavior)
    pub stream: Pin<Box<dyn Stream<Item = SendStreamingMessageResult> + Send>>,
    /// Channel receiving internal events in real-time
    pub internal_events: mpsc::UnboundedReceiver<InternalEvent>,
}
