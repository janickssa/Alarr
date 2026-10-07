//! Nextcloud (read-only): storage, active users, files, updates, PHP and database.
//!
//! Checked against the `serverinfo` app (README and `ApiController.php`):
//! `GET /ocs/v2.php/apps/serverinfo/api/v1/info?format=json&skipApps=false&skipUpdate=false`
//! with `OCS-APIRequest: true`. Auth is either an admin's username + app password (HTTP
//! Basic) or the serverinfo token in the `NC-Token` header (when no username is set).

use std::time::Instant;

use async_trait::async_trait;
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::http::{ServiceHttp, fmt_bytes, validate_http};
use super::{
    ConnectionTest, Health, InstanceConfig, ModuleData, PollOutcome, ServiceKind, ServiceModule,
    describe_error,
};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NextcloudData {
    /// Free space in the data directory, bytes.
    pub free_space: Option<u64>,
    pub users: u64,
    pub files: u64,
    pub shares: u64,
    pub active_5m: u64,
    pub active_1h: u64,
    pub active_24h: u64,
    /// Newer Nextcloud version available (from Nextcloud's own update check).
    pub update_available: Option<String>,
    pub app_updates: u64,
    pub apps_installed: Option<u64>,
    pub php_version: Option<String>,
    pub php_memory_limit: Option<u64>,
    pub database: Option<String>,
    pub database_size: Option<u64>,
    pub webserver: Option<String>,
    /// Load averages (1, 5, 15 min).
    pub cpu_load: Vec<f64>,
    /// Bytes.
    pub mem_total: Option<u64>,
    pub mem_free: Option<u64>,
}

impl NextcloudData {
    pub fn headline(&self) -> (String, Option<String>) {
        (
            format!(
                "{} active user{}",
                self.active_24h,
                if self.active_24h == 1 { "" } else { "s" }
            ),
            Some(match (&self.update_available, self.free_space) {
                (Some(v), _) => format!("update {v} available"),
                (None, Some(free)) => format!("{} free", fmt_bytes(free)),
                (None, None) => format!("{} users", self.users),
            }),
        )
    }
}

pub fn validate(cfg: &InstanceConfig) -> Result<(), String> {
    validate_http(cfg, &["password"])
}

pub struct NextcloudModule {
    http: Option<ServiceHttp>,
    username: Option<String>,
    password: String,
}

impl NextcloudModule {
    pub fn new(cfg: &InstanceConfig) -> anyhow::Result<Self> {
        validate(cfg).map_err(anyhow::Error::msg)?;
        Ok(Self {
            http: Some(ServiceHttp::new(
                cfg,
                &[("ocs-apirequest", "true".to_owned())],
            )?),
            username: cfg.setting_str("username").map(str::to_owned),
            password: cfg.secret("password").unwrap_or_default().to_owned(),
        })
    }

    pub fn demo() -> Self {
        Self {
            http: None,
            username: None,
            password: String::new(),
        }
    }

    async fn fetch(&self) -> anyhow::Result<(NextcloudData, Option<String>)> {
        let Some(http) = &self.http else {
            return Ok((demo_data(), Some("31.0.9.1".into())));
        };
        let mut req = http
            .request(Method::GET, "/ocs/v2.php/apps/serverinfo/api/v1/info")
            .query(&[
                ("format", "json"),
                ("skipApps", "false"),
                ("skipUpdate", "false"),
            ]);
        req = match &self.username {
            Some(user) => req.basic_auth(user, Some(&self.password)),
            None => req.header("NC-Token", &self.password),
        };
        let body: Value = ServiceHttp::send(req).await?.json().await?;
        let meta = body.get("ocs").and_then(|o| o.get("meta"));
        if meta.and_then(|m| m.get("status")).and_then(Value::as_str) != Some("ok") {
            anyhow::bail!(
                "{}",
                meta.and_then(|m| m.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("unexpected response")
            );
        }
        let data = body
            .get("ocs")
            .and_then(|o| o.get("data"))
            .cloned()
            .unwrap_or(Value::Null);
        let version = data
            .pointer("/nextcloud/system/version")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok((parse(&data), version))
    }
}

pub fn parse(d: &Value) -> NextcloudData {
    let u = |p: &str| d.pointer(p).and_then(Value::as_u64);
    let st = |p: &str| {
        d.pointer(p)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    // Memory is reported in KiB; -1 means unknown.
    let kib = |p: &str| {
        d.pointer(p)
            .and_then(Value::as_i64)
            .filter(|v| *v >= 0)
            .map(|v| v as u64 * 1024)
    };
    let update = d.pointer("/nextcloud/system/update");
    NextcloudData {
        free_space: u("/nextcloud/system/freespace"),
        users: u("/nextcloud/storage/num_users").unwrap_or(0),
        files: u("/nextcloud/storage/num_files").unwrap_or(0),
        shares: u("/nextcloud/shares/num_shares").unwrap_or(0),
        active_5m: u("/activeUsers/last5minutes").unwrap_or(0),
        active_1h: u("/activeUsers/last1hour").unwrap_or(0),
        active_24h: u("/activeUsers/last24hours").unwrap_or(0),
        update_available: update
            .filter(|u| u.get("available").and_then(Value::as_bool) == Some(true))
            .map(|u| {
                u.get("available_version")
                    .and_then(Value::as_str)
                    .unwrap_or("new version")
                    .to_owned()
            }),
        app_updates: u("/nextcloud/system/apps/num_updates_available").unwrap_or(0),
        apps_installed: u("/nextcloud/system/apps/num_installed"),
        php_version: st("/server/php/version"),
        php_memory_limit: u("/server/php/memory_limit"),
        database: match (st("/server/database/type"), st("/server/database/version")) {
            (Some(t), Some(v)) => Some(format!("{t} {v}")),
            (t, _) => t,
        },
        database_size: u("/server/database/size"),
        webserver: st("/server/webserver"),
        cpu_load: d
            .pointer("/nextcloud/system/cpuload")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default(),
        mem_total: kib("/nextcloud/system/mem_total"),
        mem_free: kib("/nextcloud/system/mem_free"),
    }
}

fn demo_data() -> NextcloudData {
    NextcloudData {
        free_space: Some(612 * 1024 * 1024 * 1024),
        users: 6,
        files: 184_302,
        shares: 41,
        active_5m: 1,
        active_1h: 2,
        active_24h: 4,
        update_available: Some("31.0.10".into()),
        app_updates: 2,
        apps_installed: Some(48),
        php_version: Some("8.3.12".into()),
        php_memory_limit: Some(1024 * 1024 * 1024),
        database: Some("pgsql 16.4".into()),
        database_size: Some(412 * 1024 * 1024),
        webserver: Some("Apache/2.4".into()),
        cpu_load: vec![0.42, 0.37, 0.31],
        mem_total: Some(4 * 1024 * 1024 * 1024),
        mem_free: Some(1_700 * 1024 * 1024),
    }
}

#[async_trait]
impl ServiceModule for NextcloudModule {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Nextcloud
    }

    async fn poll(&self) -> anyhow::Result<PollOutcome> {
        let started = Instant::now();
        let (data, version) = self.fetch().await?;
        Ok(PollOutcome {
            health: Health::Healthy,
            health_reason: None,
            response_ms: Some(started.elapsed().as_millis() as u64),
            version,
            data: ModuleData::Nextcloud(Box::new(data)),
        })
    }

    async fn test_connection(&self) -> ConnectionTest {
        match self.fetch().await {
            Ok((d, version)) => ConnectionTest {
                ok: true,
                message: format!("{} users, {} files", d.users, d.files),
                version,
                ..Default::default()
            },
            Err(e) => ConnectionTest {
                ok: false,
                message: describe_error(&e),
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg(url: &str, username: Option<&str>) -> InstanceConfig {
        InstanceConfig {
            id: "nextcloud".into(),
            kind: ServiceKind::Nextcloud,
            name: "Nextcloud".into(),
            base_url: Some(url.into()),
            settings: json!({"username": username}),
            secrets: [("password".to_owned(), "secret".to_owned())].into(),
        }
    }

    /// The README's example JSON with update info added.
    fn body() -> Value {
        json!({"ocs":{"meta":{"status":"ok","statuscode":200,"message":"OK"},"data":{"nextcloud":{"system":{"version":"30.0.0.1","freespace":48472944640u64,"cpuload":[0.85,1.04,1.17],"mem_total":8183664,"mem_free":5877156,"swap_total":0,"swap_free":0,"apps":{"num_installed":53,"num_updates_available":1,"app_updates":{"files_antivirus":"2.0.1"}},"update":{"lastupdatedat":1719244666,"available":true,"available_version":"30.0.1"}},"storage":{"num_users":7,"num_files":708860,"num_storages":125},"shares":{"num_shares":1}},"server":{"webserver":"Apache/2.4","php":{"version":"7.2.14","memory_limit":536870912,"max_execution_time":3600,"upload_max_filesize":535822336},"database":{"type":"mysql","version":"10.2.21","size":331382784}},"activeUsers":{"last5minutes":2,"last1hour":3,"last24hours":5}}}})
    }

    #[tokio::test]
    async fn app_password_uses_basic_auth() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ocs/v2.php/apps/serverinfo/api/v1/info"))
            .and(query_param("format", "json"))
            .and(header("ocs-apirequest", "true"))
            // admin:secret
            .and(header("authorization", "Basic YWRtaW46c2VjcmV0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body()))
            .mount(&s)
            .await;
        let m = NextcloudModule::new(&cfg(&s.uri(), Some("admin"))).unwrap();
        let out = m.poll().await.unwrap();
        let ModuleData::Nextcloud(d) = out.data else {
            panic!()
        };
        assert_eq!(out.version.as_deref(), Some("30.0.0.1"));
        assert_eq!((d.users, d.files, d.active_24h), (7, 708860, 5));
        assert_eq!(d.update_available.as_deref(), Some("30.0.1"));
        assert_eq!(d.database.as_deref(), Some("mysql 10.2.21"));
        assert_eq!(d.mem_total, Some(8183664 * 1024));
        assert_eq!(d.headline().1.as_deref(), Some("update 30.0.1 available"));
    }

    #[tokio::test]
    async fn token_goes_in_nc_token_header() {
        let s = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("nc-token", "secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body()))
            .mount(&s)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&s)
            .await;
        let m = NextcloudModule::new(&cfg(&s.uri(), None)).unwrap();
        assert!(m.poll().await.is_ok());
    }
}
