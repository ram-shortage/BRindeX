//! Windows service lifecycle management.
//!
//! This module handles the Windows service lifecycle including:
//! - Service registration and control
//! - State transitions (Starting -> Running -> Stopping -> Stopped)
//! - Configuration loading

pub mod config;
pub mod control;

// Re-exports will be added after implementation
