//! Last backup per guest, from `vzdump` tasks.
//!
//! Listing backup files would need `Datastore.AllocateSpace` and `VM.Backup`, which the
//! read-only token must not have, so this reads tasks (`Sys.Audit`) instead. A backup of one
//! guest is a `vzdump` task whose id is the vmid; a job over several guests is one task per
//! node without an id, and its log says per guest "Finished Backup of VM 101 (…)" or
//! "Backup of VM 102 failed - …" (checked against pve-manager `PVE/VZDump.pm` and
//! `PVE/API2/VZDump.pm`). Finished task logs never change, so they are cached.

use std::collections::HashMap;
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tokio::sync::Mutex;
use utoipa::ToSchema;

use super::api::{ProxmoxApi, TaskListEntry};

/// How far back to look.
pub const BACKUP_WINDOW_DAYS: i64 = 35;
/// Recompute at most this often.
const TTL: StdDuration = StdDuration::from_secs(300);
/// Job logs read per refresh at most (the rest come on the next refresh).
const MAX_LOGS_PER_REFRESH: usize = 40;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuestBackup {
    pub vmid: u32,
    pub node: String,
    /// When the backup task ended.
    pub at: Option<DateTime<Utc>>,
    pub ok: bool,
    /// Error text when it failed.
    pub error: Option<String>,
    pub upid: String,
}

/// One guest's outcome in a backup task: (vmid, ok, error).
type Outcome = (u32, bool, Option<String>);

/// Per-guest outcomes in one job log.
pub fn parse_job_log(lines: &[String]) -> Vec<Outcome> {
    let mut out = Vec::new();
    for line in lines {
        if let Some(rest) = line.split("Finished Backup of VM ").nth(1) {
            if let Some(vmid) = rest.split_whitespace().next().and_then(|v| v.parse().ok()) {
                out.push((vmid, true, None));
            }
        } else if let Some(rest) = line.split("Backup of VM ").nth(1)
            && let Some((id, err)) = rest.split_once(" failed - ")
            && let Ok(vmid) = id.trim().parse()
        {
            out.push((vmid, false, Some(err.trim().to_owned())));
        }
    }
    out
}

fn single_result(t: &TaskListEntry) -> Option<Outcome> {
    let vmid = t.id.as_deref()?.parse().ok()?;
    let status = t.status.as_deref()?;
    // `OK`, or `WARNINGS: n` (still a backup), or the error text.
    let ok = status == "OK" || status.starts_with("WARNINGS");
    Some((vmid, ok, (!ok).then(|| status.to_owned())))
}

#[derive(Default)]
pub struct BackupCache {
    logs: Mutex<HashMap<String, Vec<Outcome>>>,
    result: Mutex<Option<(Instant, HashMap<u32, GuestBackup>)>>,
}

impl BackupCache {
    /// Latest backup per vmid on the given (online) nodes, within [`BACKUP_WINDOW_DAYS`].
    pub async fn get(
        &self,
        api: &dyn ProxmoxApi,
        nodes: &[String],
    ) -> anyhow::Result<HashMap<u32, GuestBackup>> {
        let mut result = self.result.lock().await;
        if let Some((at, r)) = result.as_ref()
            && at.elapsed() < TTL
        {
            return Ok(r.clone());
        }
        let since = (Utc::now() - Duration::days(BACKUP_WINDOW_DAYS)).timestamp();
        let mut tasks: Vec<TaskListEntry> = Vec::new();
        for node in nodes {
            let mut t = api.vzdump_tasks(node, since).await?;
            for task in &mut t {
                task.node.get_or_insert_with(|| node.clone());
            }
            tasks.extend(t);
        }
        tasks.retain(|t| t.status.is_some());
        tasks.sort_by_key(|t| std::cmp::Reverse(t.endtime.or(t.starttime)));

        let mut out: HashMap<u32, GuestBackup> = HashMap::new();
        let mut fetched = 0;
        let mut logs = self.logs.lock().await;
        for t in &tasks {
            let node = t.node.clone().unwrap_or_default();
            let results = if t.id.as_deref().is_some_and(|id| !id.is_empty()) {
                single_result(t).into_iter().collect()
            } else if let Some(r) = logs.get(&t.upid) {
                r.clone()
            } else if fetched < MAX_LOGS_PER_REFRESH {
                fetched += 1;
                let r = parse_job_log(&api.task_log(&node, &t.upid).await?);
                logs.insert(t.upid.clone(), r.clone());
                r
            } else {
                continue;
            };
            for (vmid, ok, error) in results {
                out.entry(vmid).or_insert_with(|| GuestBackup {
                    vmid,
                    node: node.clone(),
                    at: t
                        .endtime
                        .or(t.starttime)
                        .and_then(|s| DateTime::from_timestamp(s as i64, 0)),
                    ok,
                    error,
                    upid: t.upid.clone(),
                });
            }
        }
        // Forget logs of tasks that left the window.
        logs.retain(|upid, _| tasks.iter().any(|t| &t.upid == upid));
        *result = Some((Instant::now(), out.clone()));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_job_logs() {
        let lines: Vec<String> = [
            "INFO: starting new backup job: vzdump --all 1",
            "INFO: Starting Backup of VM 101 (lxc)",
            "INFO: Finished Backup of VM 101 (00:00:42)",
            "INFO: Starting Backup of VM 102 (qemu)",
            "ERROR: Backup of VM 102 failed - no space left on device",
            "2026-10-07 02:00:01 INFO: Finished Backup of VM 103 (00:01:02)",
            "INFO: Backup job finished with errors",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            parse_job_log(&lines),
            vec![
                (101, true, None),
                (102, false, Some("no space left on device".into())),
                (103, true, None)
            ]
        );
    }

    #[test]
    fn single_guest_tasks() {
        let t = |status: &str| TaskListEntry {
            upid: "u".into(),
            node: None,
            kind: Some("vzdump".into()),
            id: Some("113".into()),
            user: None,
            starttime: None,
            endtime: None,
            status: Some(status.into()),
        };
        assert_eq!(single_result(&t("OK")), Some((113, true, None)));
        assert_eq!(single_result(&t("WARNINGS: 1")), Some((113, true, None)));
        assert_eq!(
            single_result(&t("job errors")),
            Some((113, false, Some("job errors".into())))
        );
    }
}
