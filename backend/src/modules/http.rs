//! Shared HTTP access for service modules: base URL, auth headers, optional
//! certificate pinning, JSON helpers and consistent error messages.

use std::time::Duration;

use anyhow::bail;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, RequestBuilder, Response};
use serde::de::DeserializeOwned;

use super::InstanceConfig;

#[derive(Clone)]
pub struct ServiceHttp {
    client: reqwest::Client,
    base: String,
    headers: HeaderMap,
}

impl ServiceHttp {
    /// Client for `cfg.base_url`, pinning `settings.tls_fingerprint` when set.
    pub fn new(cfg: &InstanceConfig, headers: &[(&'static str, String)]) -> anyhow::Result<Self> {
        let base = cfg
            .base_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("base URL is required"))?
            .trim_end_matches('/')
            .to_owned();
        let client =
            crate::tls::http_client(cfg.setting_str("tls_fingerprint"), Duration::from_secs(10))?;
        let mut map = HeaderMap::new();
        for (k, v) in headers {
            map.insert(HeaderName::from_static(k), HeaderValue::from_str(v)?);
        }
        Ok(Self {
            client,
            base,
            headers: map,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, self.url(path))
            .headers(self.headers.clone())
    }

    /// Send and fail on non-2xx with a short, user-facing message.
    pub async fn send(req: RequestBuilder) -> anyhow::Result<Response> {
        Self::send_checked(req.send().await?)
    }

    /// Fail on non-2xx with a short, user-facing message.
    pub fn send_checked(response: Response) -> anyhow::Result<Response> {
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            bail!("authentication failed (check the API key or password)");
        }
        if !status.is_success() {
            bail!(
                "HTTP {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            );
        }
        Ok(response)
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<T> {
        Ok(Self::send(self.request(Method::GET, path).query(query))
            .await?
            .json()
            .await?)
    }

    pub async fn post_json<B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> anyhow::Result<serde_json::Value> {
        let response = Self::send(self.request(Method::POST, path).json(body)).await?;
        let text = response.text().await?;
        Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::Null))
    }

    /// POST/PATCH with query parameters and no body.
    pub async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
    ) -> anyhow::Result<()> {
        Self::send(self.request(method, path).query(query)).await?;
        Ok(())
    }

    /// Fetch raw bytes (image proxy).
    pub async fn bytes(&self, path: &str) -> anyhow::Result<(bytes::Bytes, String)> {
        let response = Self::send(self.request(Method::GET, path)).await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        Ok((response.bytes().await?, content_type))
    }
}

/// Common validation for HTTP services: base URL, optional fingerprint, required secrets.
pub fn validate_http(cfg: &InstanceConfig, required_secrets: &[&str]) -> Result<(), String> {
    let url = cfg.base_url.as_deref().ok_or("base URL is required")?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("base URL must start with http:// or https://".into());
    }
    if let Some(fp) = cfg.setting_str("tls_fingerprint") {
        crate::tls::parse_fingerprint(fp).map_err(|e| e.to_string())?;
    }
    for s in required_secrets {
        if cfg.secret(s).is_none() {
            return Err(format!("{s} is required"));
        }
    }
    Ok(())
}

/// Human-readable bytes for headlines ("1.8 TB"), binary multiples like the web UI.
pub fn fmt_bytes(value: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = value as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 || i == 0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{} {}", format!("{v:.1}").trim_end_matches(".0"), UNITS[i])
    }
}

/// "38.2 MB/s"
pub fn fmt_rate(bytes_per_second: u64) -> String {
    format!("{}/s", fmt_bytes(bytes_per_second))
}

/// "38 Mbit/s" from bits per second.
pub fn fmt_bitrate(bits_per_second: u64) -> String {
    let mbit = bits_per_second as f64 / 1_000_000.0;
    if mbit >= 10.0 {
        format!("{mbit:.0} Mbit/s")
    } else {
        format!("{mbit:.1} Mbit/s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(fmt_bytes(1_979_120_929_996), "1.8 TB");
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_rate(40_055_603), "38.2 MB/s");
        assert_eq!(fmt_bitrate(38_400_000), "38 Mbit/s");
        assert_eq!(fmt_bitrate(2_500_000), "2.5 Mbit/s");
    }
}
