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
// Synchronization - Use tokio for both, just different features
// ============================================================================

pub use tokio::sync::{Mutex, RwLock};

// ============================================================================
// Channels
// ============================================================================

pub mod channel {
    pub use tokio::sync::mpsc::{
        channel, unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender,
    };
    pub use tokio::sync::oneshot;
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

// ============================================================================
// Time Operations
// ============================================================================

pub mod time {
    pub use tokio::time::{interval, sleep, timeout, Interval};
}

// ============================================================================
// File System - Works the same in WASI P1 and native
// ============================================================================

pub mod fs {
    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    pub use tokio::fs::*;

    #[cfg(all(target_os = "wasi", target_env = "p1"))]
    pub use std::fs;
}

// ============================================================================
// Blocking Operations
// ============================================================================

/// Run blocking code
/// Note: In WASI, this doesn't actually use a thread pool

#[derive(thiserror::Error, Debug)]
pub enum BlockingError {
    #[error("task panicked")]
    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    Join(#[from] tokio::task::JoinError),
}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub async fn spawn_blocking<F, R>(f: F) -> Result<R, BlockingError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    Ok(tokio::task::spawn_blocking(f).await?)
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub async fn spawn_blocking<F, R>(f: F) -> Result<R, BlockingError>
where
    F: FnOnce() -> R + 'static,
    R: 'static,
{
    // In WASI without threads, just run it directly
    Ok(f())
}

// ============================================================================
// Random Number Generation
// ============================================================================

pub mod rand {
    /// Fill a buffer with random bytes
    pub fn fill_bytes(buf: &mut [u8]) {
        #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
        {
            use rand::RngCore;
            rand::thread_rng().fill_bytes(buf);
        }

        #[cfg(all(target_os = "wasi", target_env = "p1"))]
        {
            // WASI supports getrandom
            getrandom::getrandom(buf).expect("failed to generate random bytes");
        }
    }

    /// Generate random bytes
    #[must_use]
    pub fn random_bytes(len: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; len];
        fill_bytes(&mut bytes);
        bytes
    }
}

// ============================================================================
// Instant - std::time::Instant works in WASI P1
// ============================================================================

pub use std::time::Instant;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    #[tokio::test(flavor = "current_thread")]
    async fn spawn_executes_future() {
        let handle = spawn(async { 42u8 });
        assert_eq!(handle.await.expect("task join"), 42);
    }

    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    #[tokio::test(flavor = "current_thread")]
    async fn spawn_blocking_executes_closure() {
        let result = spawn_blocking(|| 7_u32 * 6_u32)
            .await
            .expect("blocking work");
        assert_eq!(result, 42);
    }

    #[test]
    fn random_bytes_returns_requested_length() {
        let first = rand::random_bytes(16);
        let second = rand::random_bytes(16);
        assert_eq!(first.len(), 16);
        assert_eq!(second.len(), 16);
        // The chance of equality is astronomically low, so treat equality as failure.
        assert_ne!(first, second);
    }
}
