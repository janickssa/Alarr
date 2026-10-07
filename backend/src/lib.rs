//! Quorum server library: config, storage, auth, service modules, pollers and the HTTP API.

pub mod alerts;
pub mod api;
pub mod app;
pub mod auth;
pub mod automation;
pub mod config;
pub mod crypto;
pub mod db;
pub mod events;
pub mod modules;
pub mod poller;
pub mod services;
pub mod state;
pub mod tls;
pub mod web;

/// Product name. Kept in one place so a rename is cheap.
pub const APP_NAME: &str = "Quorum";

/// Server version, taken from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
