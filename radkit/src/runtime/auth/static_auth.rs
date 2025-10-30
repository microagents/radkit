//! Static authentication service implementation.

use crate::runtime::auth::AuthService;
use crate::runtime::context::AuthContext;

/// A simple static authentication service.
///
/// This service returns a fixed [`AuthContext`] that is set at construction time.
/// It's useful for local development, testing, and single-tenant deployments where
/// authentication is handled externally.
///
/// # Examples
///
/// ```
/// use radkit::runtime::auth::{AuthService, StaticAuthService};
///
/// let auth_service = StaticAuthService::new("my-app", "alice");
/// let auth_ctx = auth_service.get_auth_context();
///
/// assert_eq!(auth_ctx.app_name, "my-app");
/// assert_eq!(auth_ctx.user_name, "alice");
/// ```
#[derive(Debug, Clone)]
pub struct StaticAuthService {
    context: AuthContext,
}

impl StaticAuthService {
    /// Creates a new static authentication service with the given app and user names.
    ///
    /// # Arguments
    ///
    /// * `app_name` - The application or agent name
    /// * `user_name` - The user name
    ///
    /// # Examples
    ///
    /// ```
    /// use radkit::runtime::auth::StaticAuthService;
    ///
    /// let auth_service = StaticAuthService::new("my-app", "alice");
    /// ```
    pub fn new(app_name: impl Into<String>, user_name: impl Into<String>) -> Self {
        Self {
            context: AuthContext {
                app_name: app_name.into(),
                user_name: user_name.into(),
            },
        }
    }

    /// Creates a new static authentication service with default values.
    ///
    /// Uses `"default-app"` as the app name and `"default-user"` as the user name.
    ///
    /// # Examples
    ///
    /// ```
    /// use radkit::runtime::auth::StaticAuthService;
    ///
    /// let auth_service = StaticAuthService::default();
    /// ```
    #[must_use] pub fn default() -> Self {
        Self::new("default-app", "default-user")
    }
}

impl AuthService for StaticAuthService {
    fn get_auth_context(&self) -> AuthContext {
        self.context.clone()
    }
}

impl Default for StaticAuthService {
    fn default() -> Self {
        Self::default()
    }
}
