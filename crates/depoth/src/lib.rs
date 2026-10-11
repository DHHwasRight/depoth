//! Depoth: a static content framework.
//!
//! Templates compile; content does not. There is no backend.

pub mod config;
pub mod frontmatter;
pub mod nav;

pub use config::{Config, Mode};
pub use nav::Navigation;
