use crate::{Hub, HubError, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicy {
    pub keep_recent: u32,
    pub daily_days: u32,
    pub weekly_weeks: u32,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct RetentionItem {
    pub backup_id: String,
    pub display_name: String,
    pub created_at: String,
    pub total_bytes: String,
    #[ts(type = "number")]
    pub metadata_revision: i64,
    pub cached_verification: String,
    pub consistency: String,
    pub protected: bool,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct RetentionPreview {
    pub generated_at: String,
    pub timezone: String,
    pub scope: String,
    pub policy: RetentionPolicy,
    pub items: Vec<RetentionItem>,
    pub protected_count: u32,
    pub candidate_count: u32,
    pub candidate_logical_bytes: String,
    pub version_context: String,
    pub deletion_available: bool,
    pub warnings: Vec<String>,
}
#[derive(sqlx::FromRow)]
pub(crate) struct Snapshot {
    id: String,
    display_name: String,
    created_at: String,
    total_bytes: String,
    metadata_revision: i64,
    retention_locked: bool,
    verification: String,
    consistency: String,
    referenced: bool,
}
fn invalid() -> HubError {
    HubError::new(
        "RETENTION_METADATA_INVALID",
        "备份日期或容量记录无效，无法生成完整保留预览。",
    )
}
fn monday(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday().into())
}
fn calculate(
    mut rows: Vec<Snapshot>,
    policy: RetentionPolicy,
    at: DateTime<Utc>,
) -> Result<RetentionPreview> {
    if policy.keep_recent > 1000 || policy.daily_days > 366 || policy.weekly_weeks > 104 {
        return Err(HubError::new(
            "RETENTION_POLICY_INVALID",
            "最近份数上限 1000、每日窗口上限 366 天、每周窗口上限 104 周；0 表示停用该条规则。",
        ));
    }
    let dates = rows
        .iter()
        .map(|row| {
            DateTime::parse_from_rfc3339(&row.created_at)
                .map(|date| (row.id.clone(), date.with_timezone(&Utc)))
                .map_err(|_| invalid())
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    rows.sort_by(|a, b| {
        dates[&b.id]
            .cmp(&dates[&a.id])
            .then_with(|| a.id.cmp(&b.id))
    });
    let today = at.date_naive();
    let week = monday(today);
    let mut days = BTreeSet::new();
    let mut weeks = BTreeSet::new();
    let mut latest_verified = false;
    let mut latest_quiescent = false;
    let mut recent_count = 0_u32;
    let mut protected_count = 0_u32;
    let mut candidate_logical_bytes = 0_u64;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let date = dates[&row.id];
        let bytes = row.total_bytes.parse::<u64>().map_err(|_| invalid())?;
        if bytes.to_string() != row.total_bytes {
            return Err(invalid());
        }
        let mut reasons = vec![];
        if !row.display_name.is_empty() {
            reasons.push("named".into());
        }
        if row.retention_locked {
            reasons.push("locked".into());
        }
        if row.referenced {
            reasons.push("restore_reference".into());
        }
        if row.verification != "verified" {
            reasons.push("not_verified".into());
        }
        if date > at {
            reasons.push("future_timestamp".into());
        } else {
            if recent_count < policy.keep_recent {
                reasons.push("recent".into());
                recent_count += 1;
            }
            if row.verification == "verified" {
                if !latest_verified {
                    reasons.push("latest_verified".into());
                    latest_verified = true;
                }
                if row.consistency == "quiescent_checked" && !latest_quiescent {
                    reasons.push("latest_quiescent".into());
                    latest_quiescent = true;
                }
                let age = (today - date.date_naive()).num_days();
                if age < i64::from(policy.daily_days) && days.insert(date.date_naive()) {
                    reasons.push("daily".into());
                }
                let age = (week - monday(date.date_naive())).num_weeks();
                if age < i64::from(policy.weekly_weeks) && weeks.insert(monday(date.date_naive())) {
                    reasons.push("weekly".into());
                }
            }
        }
        let protected = !reasons.is_empty();
        if protected {
            protected_count += 1;
        } else {
            candidate_logical_bytes = candidate_logical_bytes
                .checked_add(bytes)
                .ok_or_else(invalid)?;
        }
        items.push(RetentionItem {
            backup_id: row.id,
            display_name: row.display_name,
            created_at: row.created_at,
            total_bytes: row.total_bytes,
            metadata_revision: row.metadata_revision,
            cached_verification: row.verification,
            consistency: row.consistency,
            protected,
            reasons,
        });
    }
    Ok(RetentionPreview {
        generated_at: at.to_rfc3339(),
        timezone: "UTC".into(),
        scope: "registered_save_root".into(),
        policy,
        candidate_count: items.len() as u32 - protected_count,
        protected_count,
        candidate_logical_bytes: candidate_logical_bytes.to_string(),
        items,
        version_context: "unverified".into(),
        deletion_available: false,
        warnings: vec![
            "这是元数据规则模拟，不读取介质、不重新校验归档，也不执行删除。".into(),
            "容量为源文件逻辑字节数，不是归档占用或可释放空间。".into(),
            "当前仅覆盖已登记的手动存档根目录备份；最近份数不是自动检查点保留策略。".into(),
            "日/周窗口按 UTC 计算，周从周一开始；尚未开放本地时区和持久策略设置。".into(),
            "游戏版本、存档环境和升级前保护点关联尚未验证；不能据此认定跨版本可回滚或授权删除。"
                .into(),
            "verified 仅为历史校验状态，介质离线或损坏时不能充当当前有效保底。".into(),
        ],
    })
}
impl Hub {
    pub async fn preview_retention(&self, policy: RetentionPolicy) -> Result<RetentionPreview> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let workspace: Option<String> = sqlx::query_scalar("SELECT id FROM workspace LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .map_err(HubError::db)?;
        let workspace = workspace.ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let rows=sqlx::query_as::<_,Snapshot>("SELECT b.id,b.display_name,b.created_at,b.total_bytes,b.metadata_revision,b.retention_locked,b.verification,b.consistency,COALESCE(refs.referenced,0) AS referenced FROM backup_archive b LEFT JOIN (SELECT backup_id,1 AS referenced FROM restore_task GROUP BY backup_id) refs ON refs.backup_id=b.id WHERE b.workspace_id=? ORDER BY b.id LIMIT 5001").bind(workspace).fetch_all(&mut *tx).await.map_err(HubError::db)?;
        if rows.len() > 5000 {
            return Err(HubError::new(
                "RETENTION_LIMIT_EXCEEDED",
                "超过当前 5000 条完整预览上限；未生成截断的清理候选。",
            ));
        }
        tx.commit().await.map_err(HubError::db)?;
        calculate(rows, policy, Utc::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: &str, date: &str) -> Snapshot {
        Snapshot {
            id: id.into(),
            display_name: String::new(),
            created_at: date.into(),
            total_bytes: "10".into(),
            metadata_revision: 0,
            retention_locked: false,
            verification: "verified".into(),
            consistency: "best_effort".into(),
            referenced: false,
        }
    }
    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn zero() -> RetentionPolicy {
        RetentionPolicy {
            keep_recent: 0,
            daily_days: 0,
            weekly_weeks: 0,
        }
    }
    #[test]
    fn protects_named_locked_references_and_last_verified_with_rules_disabled() {
        let mut named = row("named", "2025-01-01T00:00:00Z");
        named.display_name = "升级前".into();
        let mut locked = row("locked", "2025-01-02T00:00:00Z");
        locked.retention_locked = true;
        let mut reference = row("reference", "2025-01-03T00:00:00Z");
        reference.referenced = true;
        let result = calculate(
            vec![
                named,
                locked,
                reference,
                row("last", "2026-01-05T00:00:00Z"),
                row("candidate", "2025-01-04T00:00:00Z"),
            ],
            zero(),
            at(),
        )
        .unwrap();
        assert_eq!(result.protected_count, 4);
        assert_eq!(result.candidate_count, 1);
        assert_eq!(result.candidate_logical_bytes, "10");
        assert!(!result.deletion_available);
        assert_eq!(result.version_context, "unverified");
    }
    #[test]
    fn daily_and_iso_week_buckets_use_latest_verified_and_union_with_recent() {
        let rows = vec![
            row("mon-new", "2026-01-05T11:00:00Z"),
            row("mon-old", "2026-01-05T10:00:00Z"),
            row("sun", "2026-01-04T23:00:00Z"),
            row("previous-week", "2025-12-28T23:00:00Z"),
        ];
        let result = calculate(
            rows,
            RetentionPolicy {
                keep_recent: 2,
                daily_days: 2,
                weekly_weeks: 2,
            },
            at(),
        )
        .unwrap();
        assert_eq!(result.protected_count, 3);
        assert_eq!(result.candidate_count, 1);
        assert_eq!(
            result
                .items
                .iter()
                .find(|row| row.backup_id == "sun")
                .unwrap()
                .reasons,
            vec!["daily", "weekly"]
        );
    }
    #[test]
    fn future_and_unverified_records_never_displace_verified_or_quiescent_anchors() {
        let mut bad = row("bad", "2026-01-05T11:00:00Z");
        bad.verification = "unavailable".into();
        let mut strong = row("strong", "2025-01-01T00:00:00Z");
        strong.consistency = "quiescent_checked".into();
        let result = calculate(
            vec![
                row("future", "2026-01-06T00:00:00Z"),
                bad,
                row("latest", "2026-01-05T10:00:00Z"),
                strong,
            ],
            zero(),
            at(),
        )
        .unwrap();
        assert_eq!(result.protected_count, 4);
        assert!(
            result
                .items
                .iter()
                .find(|item| item.backup_id == "strong")
                .unwrap()
                .reasons
                .contains(&"latest_quiescent".into())
        );
    }
    #[test]
    fn parses_actual_instants_instead_of_lexicographic_offsets() {
        let result = calculate(
            vec![
                row("older", "2026-01-05T12:00:00+02:00"),
                row("newer", "2026-01-05T11:00:00Z"),
            ],
            zero(),
            at(),
        )
        .unwrap();
        assert_eq!(result.items[0].backup_id, "newer");
        assert!(result.items[0].protected);
    }
    #[test]
    fn malformed_metadata_and_oversized_rules_fail_closed() {
        let mut bad = row("bad", "invalid");
        assert!(calculate(vec![bad], zero(), at()).is_err());
        bad = row("bad", "2026-01-01T00:00:00Z");
        bad.total_bytes = "01".into();
        assert!(calculate(vec![bad], zero(), at()).is_err());
        assert_eq!(
            calculate(
                vec![],
                RetentionPolicy {
                    keep_recent: 1001,
                    ..zero()
                },
                at()
            )
            .unwrap_err()
            .code,
            "RETENTION_POLICY_INVALID"
        );
    }
}
