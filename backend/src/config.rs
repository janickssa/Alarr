//! Server configuration: `quorum.toml` plus environment overrides.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine;
use serde::Deserialize;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/quorum/quorum.toml";
pub const DEFAULT_SEED_PATH: &str = "/etc/quorum/services.seed.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub ntfy: Option<NtfyConfig>,
    pub apns: Option<ApnsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: SocketAddr,
    /// External URL clients use (put in pairing QR codes; `https` enables `Secure` cookies).
    pub public_url: Option<String>,
    #[serde(default = "default_database")]
    pub database: PathBuf,
    /// Optional services seed file, imported once when no services exist.
    pub seed: Option<PathBuf>,
}

/// ntfy delivery (used from phase 4).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NtfyConfig {
    pub url: String,
    pub topic: String,
}

/// APNs delivery (used from phase 6).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApnsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub team_id: String,
    pub key_id: String,
    pub key_path: PathBuf,
    pub bundle_id: String,
    #[serde(default = "default_apns_env")]
    pub environment: String,
}

fn default_bind() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 8080))
}

fn default_database() -> PathBuf {
    PathBuf::from("/var/lib/quorum/quorum.db")
}

fn default_apns_env() -> String {
    "production".to_owned()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            public_url: None,
            database: default_database(),
            seed: None,
        }
    }
}

impl Config {
    /// Load from `QUORUM_CONFIG` (or the default path), then apply environment overrides.
    /// A missing file at the default path is allowed (defaults are used).
    pub fn load() -> anyhow::Result<Self> {
        let explicit = std::env::var_os("QUORUM_CONFIG").map(PathBuf::from);
        let path = explicit
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
        let mut config = match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).with_context(|| format!("in {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && explicit.is_none() => {
                tracing::warn!(path = %path.display(), "config file not found, using defaults");
                Self::parse("")?
            }
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        config.apply_env()?;
        Ok(config)
    }

    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    fn apply_env(&mut self) -> anyhow::Result<()> {
        if let Ok(bind) = std::env::var("QUORUM_BIND") {
            self.server.bind = bind.parse().context("QUORUM_BIND")?;
        }
        if let Ok(db) = std::env::var("QUORUM_DATABASE") {
            self.server.database = PathBuf::from(db);
        }
        if let Ok(url) = std::env::var("QUORUM_PUBLIC_URL") {
            self.server.public_url = Some(url);
        }
        Ok(())
    }

    /// The seed file to import on first start, if any.
    pub fn seed_path(&self) -> Option<PathBuf> {
        match &self.server.seed {
            Some(p) => Some(p.clone()),
            None => Some(PathBuf::from(DEFAULT_SEED_PATH)).filter(|p| Path::new(p).exists()),
        }
    }

    pub fn secure_cookies(&self) -> bool {
        self.server
            .public_url
            .as_deref()
            .is_some_and(|u| u.starts_with("https://"))
    }
}

/// Read the 32-byte master key from `QUORUM_SECRET_KEY` (base64).
pub fn secret_key_from_env() -> anyhow::Result<[u8; 32]> {
    let Ok(value) = std::env::var("QUORUM_SECRET_KEY") else {
        bail!("QUORUM_SECRET_KEY is not set. Generate one with: openssl rand -base64 32");
    };
    parse_secret_key(&value)
}

pub fn parse_secret_key(value: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .context("QUORUM_SECRET_KEY is not valid base64")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("QUORUM_SECRET_KEY must decode to exactly 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_readme_example() {
        let config = Config::parse(
            r#"
            [server]
            bind = "0.0.0.0:8080"
            public_url = "https://quorum.example.ts.net"
            database = "/var/lib/quorum/quorum.db"

            [ntfy]
            url = "https://ntfy.example.lan"
            topic = "quorum"

            [apns]
            enabled = true
            team_id = "ABCDE12345"
            key_id = "XYZ987WVUT"
            key_path = "/etc/quorum/AuthKey_XYZ987WVUT.p8"
            bundle_id = "com.example.quorum"
            environment = "production"
            "#,
        )
        .unwrap();
        assert_eq!(config.server.bind.port(), 8080);
        assert!(config.secure_cookies());
        assert_eq!(config.ntfy.unwrap().topic, "quorum");
    }

    #[test]
    fn empty_config_uses_defaults() {
        let config = Config::parse("").unwrap();
        assert_eq!(config.server.bind, default_bind());
        assert!(!config.secure_cookies());
    }

    #[test]
    fn secret_key_must_be_32_bytes() {
        assert!(parse_secret_key("c2hvcnQ=").is_err());
        let key = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        assert_eq!(parse_secret_key(&key).unwrap(), [7u8; 32]);
    }
}
