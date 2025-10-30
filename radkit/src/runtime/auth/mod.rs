//! Authentication and tenancy service.
//!
//! This module provides authentication and multi-tenancy support for agent execution.
//! The [`AuthService`] trait defines the interface for obtaining the current authentication
//! context, which is used by other runtime services to namespace operations.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::runtime::auth::{AuthService, StaticAuthService};
//!
//! let auth_service = StaticAuthService::new("my-app", "alice");
//! let auth_ctx = auth_service.get_auth_context();
//! println!("App: {}, User: {}", auth_ctx.app_name, auth_ctx.user_name);
//! ```

pub mod static_auth;

pub use static_auth::StaticAuthService;

use crate::compat::{MaybeSend, MaybeSync};
use crate::runtime::context::AuthContext;

/// Service for providing authentication and tenancy context.
///
/// Implementations of this trait are responsible for identifying the current
/// user and tenant (application) for any given request. This is fundamental
/// for security and multi-tenancy, ensuring that one user or application
/// cannot access another's data.
///
/// # Multi-tenancy
///
/// The [`AuthContext`] returned by this service is used by other runtime
/// services (like [`MemoryService`](crate::runtime::memory::MemoryService) and
/// [`TaskManager`](crate::runtime::task_manager::TaskManager)) to namespace
/// their operations, guaranteeing data isolation.
pub trait AuthService: MaybeSend + MaybeSync {
    /// Returns the current authentication context.
    ///
    /// This method should return the authentication information for the
    /// current request or execution context.
    fn get_auth_context(&self) -> AuthContext;
}
