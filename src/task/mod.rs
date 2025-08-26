//! Task Management System
//!
//! This module provides the core task management functionality for the AK-Rust SDK.
//! It separates task lifecycle management from session management, providing:
//!
//! - `TaskStore`: Database-ready abstraction for task persistence with app/user security
//! - `TaskManager`: High-level task operations and A2A event generation  
//! - `InMemoryTaskStore`: In-memory implementation for development/testing
//!
//! Key features:
//! - Direct A2A Task type usage (no conversion overhead)
//! - Multi-tenant security isolation at the database layer
//! - Built for database backends with TaskStore trait
//! - A2A protocol compliant events

pub mod in_memory_task_store;
pub mod task_manager;
pub mod task_store;

pub use in_memory_task_store::InMemoryTaskStore;
pub use task_manager::TaskManager;
pub use task_store::TaskStore;
