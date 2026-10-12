//! Depoth: a static content framework.
//!
//! Templates compile; content does not. There is no backend.

pub mod assets;
pub mod cache;
pub mod components;
pub mod config;
pub mod dev;
pub mod feeds;
pub mod frontmatter;
pub mod images;
pub mod markdown;
pub mod nav;
pub mod routes;
pub mod search;
pub mod slug;

pub use config::{Config, Mode};
pub use nav::Navigation;
