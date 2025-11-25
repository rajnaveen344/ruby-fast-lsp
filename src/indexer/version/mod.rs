//! Ruby Version Management
//!
//! This module handles Ruby version detection and version manager integration.
//!
//! ## Components
//!
//! - **`version_detector`**: Detects Ruby version from workspace files (.ruby-version, Gemfile, etc.)
//! - **`version_managers`**: Interfaces with rbenv, rvm, chruby, and system Ruby

pub mod version_detector;
pub mod version_managers;
