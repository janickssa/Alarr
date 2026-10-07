//! Quorum server library: HTTP API, OpenAPI document and (later) pollers and modules.

pub mod api;

/// Product name. Kept in one place so a rename is cheap.
pub const APP_NAME: &str = "Quorum";

/// Server version, taken from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
