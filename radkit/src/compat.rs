//! WASI P1 compatibility helpers
//!
//! This provides a consistent API between native and wasm32-wasi targets.

use std::future::Future;
use std::pin::Pin;

// For WASI P1, everything is single-threaded, so no Send/Sync
#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub trait MaybeSend {}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub trait MaybeSend: Send {}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
impl<T: ?Sized> MaybeSend for T {}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
impl<T: Send + ?Sized> MaybeSend for T {}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub trait MaybeSync {}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub trait MaybeSync: Sync {}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
impl<T: ?Sized> MaybeSync for T {}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
impl<T: Sync + ?Sized> MaybeSync for T {}

// ============================================================================
// Type Aliases
// ============================================================================

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub type MaybeSendBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub type MaybeSendBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub type MaybeSendBoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub type MaybeSendBoxError = Box<dyn std::error::Error + 'static>;

// ============================================================================
// Channels
// ============================================================================

pub mod channel {
    pub use tokio::sync::mpsc::{
        channel, unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender,
    };
}

// ============================================================================
// Task Spawning
// ============================================================================

/// Spawn a task on the tokio runtime
#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future)
}

/// Spawn a task on the tokio runtime
#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    tokio::task::spawn_local(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    #[tokio::test(flavor = "current_thread")]
    async fn spawn_executes_future() {
        let handle = spawn(async { 42u8 });
        assert_eq!(handle.await.expect("task join"), 42);
    }
}
