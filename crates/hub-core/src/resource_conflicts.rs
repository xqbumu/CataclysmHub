use crate::{Hub, HubError, ResourceClaim, Result, database::now, paths};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct ResourceScope {
    pub role: String,
    pub path: String,
    pub access: String,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ResourceScopeIssue {
    pub path: String,
    pub code: String,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ResourceConflict {
    pub owner_kind: String,
    pub owner_id: String,
    pub operation: String,
    pub state: String,
    pub reason: String,
    pub requested_path: Option<String>,
    pub protected_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct InstanceResourcePreview {
    pub instance_id: String,
    pub checked_at: String,
    pub scopes: Vec<ResourceScope>,
    pub scope_issues: Vec<ResourceScopeIssue>,
    pub conflicts: Vec<ResourceConflict>,
    pub binding_verified: bool,
    pub concurrency_available: bool,
}

// Keep handles alive during comparisons. Canonical names alone miss aliases on
// case-insensitive filesystems. This is an observation, not an acquired lease.
fn inspect(path: &str) -> Result<std::path::PathBuf> {
    paths::canonical(path, true).map_err(|_| {
        HubError::new(
            "RESOURCE_SCOPE_UNAVAILABLE",
            "资源目录不存在、类型错误或无法访问。",
        )
    })
}

fn compare(
    instance_id: String,
    scopes: Vec<ResourceScope>,
    claims: Vec<ResourceClaim>,
) -> Result<InstanceResourcePreview> {
    // Reject excess input rather than reporting a partial absence of conflicts.
    if claims.len() > 10_000 || scopes.len() > 1_000 {
        return Err(HubError::new(
            "RESOURCE_SCOPE_LIMIT",
            "活动资源范围过多，无法完成检查。",
        ));
    }
    let mut issues = Vec::new();
    let mut requested = Vec::new();
    if scopes.is_empty() {
        return Err(HubError::new(
            "RESOURCE_SCOPE_UNAVAILABLE",
            "请求缺少资源范围。",
        ));
    }
    for scope in &scopes {
        if !matches!(scope.access.as_str(), "read" | "write") {
            issues.push(ResourceScopeIssue {
                path: scope.path.clone(),
                code: "RESOURCE_ACCESS_INVALID".into(),
            });
            continue;
        }
        match inspect(&scope.path) {
            Ok(path) => requested.push((scope, path)),
            Err(error) => issues.push(ResourceScopeIssue {
                path: scope.path.clone(),
                code: error.code.into(),
            }),
        }
    }
    for (index, (scope, path)) in requested.iter().enumerate() {
        for (_, other) in requested.iter().skip(index + 1) {
            match crate::play_instances::overlaps(path, other) {
                Ok(true) => issues.push(ResourceScopeIssue {
                    path: scope.path.clone(),
                    code: "REQUEST_SCOPE_OVERLAP".into(),
                }),
                Ok(false) => (),
                Err(_) => issues.push(ResourceScopeIssue {
                    path: scope.path.clone(),
                    code: "RESOURCE_SCOPE_UNAVAILABLE".into(),
                }),
            }
        }
    }
    let mut conflicts = Vec::new();
    for claim in claims {
        let conflict = |reason: &str, requested_path| ResourceConflict {
            owner_kind: claim.owner_kind.clone(),
            owner_id: claim.owner_id.clone(),
            operation: claim.operation.clone(),
            state: claim.state.clone(),
            reason: reason.into(),
            requested_path,
            protected_path: claim.path.clone(),
        };
        if claim.scope == "global" && claim.access == "exclusive" {
            conflicts.push(conflict(&claim.reason, None));
            continue;
        }
        if claim.scope != "directory" || !matches!(claim.access.as_str(), "read" | "write") {
            conflicts.push(conflict("scope_unresolved", None));
            continue;
        }
        let Some(path) = claim.path.as_deref() else {
            conflicts.push(conflict("scope_unresolved", None));
            continue;
        };
        let Ok(protected) = inspect(path) else {
            conflicts.push(conflict("scope_unresolved", None));
            continue;
        };
        for (scope, requested_path) in &requested {
            if scope.access == "read" && claim.access == "read" {
                continue;
            }
            match crate::play_instances::overlaps(requested_path, &protected) {
                Ok(true) => conflicts.push(conflict("directory_overlap", Some(scope.path.clone()))),
                Ok(false) => (),
                Err(_) => conflicts.push(conflict("scope_unresolved", Some(scope.path.clone()))),
            }
        }
    }
    Ok(InstanceResourcePreview {
        instance_id,
        checked_at: now(),
        scopes,
        scope_issues: issues,
        conflicts,
        binding_verified: false,
        concurrency_available: false,
    })
}

impl Hub {
    /// Read-only observation; never reserves resources or upgrades binding evidence.
    pub async fn preview_instance_resources(
        &self,
        instance_id: &str,
    ) -> Result<InstanceResourcePreview> {
        let _gate = self.mutation.lock().await;
        self.instance_resources_inner(instance_id).await
    }

    pub(crate) async fn instance_resources_inner(
        &self,
        instance_id: &str,
    ) -> Result<InstanceResourcePreview> {
        let instance = self.instance(instance_id).await?;
        let installation = self.installation(&instance.installation_id).await?;
        let scopes = vec![
            ResourceScope {
                role: "installation".into(),
                path: crate::paths::text(&crate::paths::installation_scope(
                    &installation.working_dir,
                ))?,
                access: "write".into(),
            },
            ResourceScope {
                role: "instance".into(),
                path: instance.data_root,
                access: "write".into(),
            },
        ];
        let mut connection = self.pool.acquire().await.map_err(HubError::db)?;
        let mut claims = crate::resource_claims::read_claims(&mut connection).await?;
        drop(connection);
        self.overlay_claims(&mut claims).await;
        let instance_id = instance_id.to_owned();
        tokio::task::spawn_blocking(move || compare(instance_id, scopes, claims))
            .await
            .map_err(|_| HubError::new("RESOURCE_CHECK_FAILED", "资源冲突检查未完成。"))?
    }

    // Caller holds mutation through durable task admission. Only supervised
    // candidates lose global claims; unknown games and every file task retain them.
    pub(crate) async fn check_file_transfer_resources(
        &self,
        source: &str,
        destination: &str,
    ) -> Result<()> {
        let scopes = vec![
            ResourceScope {
                role: "source".into(),
                path: source.into(),
                access: "read".into(),
            },
            ResourceScope {
                role: "destination".into(),
                path: destination.into(),
                access: "write".into(),
            },
        ];
        let mut connection = self.pool.acquire().await.map_err(HubError::db)?;
        let mut claims = crate::resource_claims::read_claims(&mut connection).await?;
        drop(connection);
        self.overlay_claims(&mut claims).await;
        // Preserve global-owner errors before examining conservative parent scopes.
        if claims.iter().any(|claim| claim.scope == "global") {
            let code = if claims.iter().any(|claim| claim.owner_kind == "session") {
                "GAME_RUNNING_OR_UNKNOWN"
            } else {
                "TASK_CONFLICT"
            };
            return Err(HubError::new(
                code,
                "存在活动任务或未知游戏写入范围，操作已阻止。",
            ));
        }
        // With no active owners, preserve the operation's existing path rules.
        // Restore may create a fresh child under a common ancestor of its source.
        if claims.is_empty() {
            return Ok(());
        }
        tokio::task::spawn_blocking(move || compare(String::new(), scopes, claims))
            .await
            .map_err(|_| HubError::new("RESOURCE_CHECK_FAILED", "资源冲突检查未完成。"))??
            .ensure_admissible()
    }

    pub(crate) async fn check_instance_resources(&self, instance_id: &str) -> Result<()> {
        let preview = self.instance_resources_inner(instance_id).await?;
        preview.ensure_admissible()
    }
}

impl InstanceResourcePreview {
    // Convert the scope report into an execution decision. A clear report is not
    // evidence that a game is eligible for scoped concurrency.
    fn ensure_admissible(&self) -> Result<()> {
        if self
            .scope_issues
            .iter()
            .any(|issue| issue.code == "REQUEST_SCOPE_OVERLAP")
        {
            return Err(HubError::new(
                "INSTANCE_SCOPE_OVERLAP",
                "请求目录范围共享或嵌套，操作已阻止。",
            ));
        }
        if self
            .scope_issues
            .iter()
            .any(|issue| issue.code == "RESOURCE_ACCESS_INVALID")
        {
            return Err(HubError::new(
                "RESOURCE_ACCESS_INVALID",
                "资源访问模式无法识别，操作已阻止。",
            ));
        }
        if !self.scope_issues.is_empty() {
            return Err(HubError::new(
                "RESOURCE_SCOPE_UNAVAILABLE",
                "请求目录不可访问，操作已阻止。",
            ));
        }
        if let Some(conflict) = self.conflicts.first() {
            let code = if self.conflicts.iter().any(|c| c.owner_kind == "session") {
                "GAME_RUNNING_OR_UNKNOWN"
            } else {
                "TASK_CONFLICT"
            };
            let message = if self
                .conflicts
                .iter()
                .any(|c| c.reason == "directory_overlap")
            {
                "请求目录与活动操作共享或嵌套，操作已阻止。"
            } else if conflict.reason == "scope_unresolved" {
                "活动资源范围无法核实，操作已阻止。"
            } else {
                "存在活动操作或未知游戏写入范围，操作已阻止。"
            };
            return Err(HubError::new(code, message));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn scope(path: &Path, access: &str) -> ResourceScope {
        ResourceScope {
            role: "instance".into(),
            path: paths::text(path).unwrap(),
            access: access.into(),
        }
    }
    fn claim(path: Option<&Path>, access: &str) -> ResourceClaim {
        ResourceClaim {
            owner_kind: "backup".into(),
            owner_id: "owner".into(),
            operation: "backup.create".into(),
            state: "running".into(),
            scope: "directory".into(),
            access: access.into(),
            path: path.map(|p| paths::text(p).unwrap()),
            reason: "declared_directory".into(),
        }
    }
    fn check(scopes: Vec<ResourceScope>, claims: Vec<ResourceClaim>) -> InstanceResourcePreview {
        compare("instance".into(), scopes, claims).unwrap()
    }

    #[test]
    fn compares_both_ancestor_directions_and_keeps_read_read_nonconflicting() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let child = root.join("child");
        let sibling = temp.path().join("sibling");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::create_dir(&sibling).unwrap();
        for (requested, protected) in [(&root, &root), (&root, &child), (&child, &root)] {
            for (left, right, expected) in [
                ("read", "read", 0),
                ("write", "read", 1),
                ("read", "write", 1),
                ("write", "write", 1),
            ] {
                let report = check(
                    vec![scope(requested, left)],
                    vec![claim(Some(protected), right)],
                );
                assert_eq!(report.conflicts.len(), expected);
                assert!(!report.binding_verified && !report.concurrency_available);
            }
        }
        assert!(
            check(
                vec![scope(&root, "write")],
                vec![claim(Some(&sibling), "write")]
            )
            .conflicts
            .is_empty()
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_aliases_cannot_hide_a_shared_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("real");
        let alias = temp.path().join("alias");
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        for (a, b) in [(&root, &alias), (&alias, &root)] {
            let report = check(vec![scope(a, "write")], vec![claim(Some(b), "write")]);
            assert_eq!(report.conflicts[0].reason, "directory_overlap");
        }
        assert!(std::fs::read_dir(&root).unwrap().next().is_none());
    }

    #[test]
    fn missing_wrong_type_and_malformed_scopes_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        let file = temp.path().join("file");
        std::fs::write(&file, b"untouched").unwrap();
        for invalid in [&missing, &file] {
            let report = check(
                vec![scope(temp.path(), "write")],
                vec![claim(Some(invalid), "read")],
            );
            assert_eq!(report.conflicts[0].reason, "scope_unresolved");
            let report = check(vec![scope(invalid, "write")], vec![]);
            assert_eq!(report.scope_issues[0].code, "RESOURCE_SCOPE_UNAVAILABLE");
        }
        for mut invalid in [claim(None, "write"), claim(Some(temp.path()), "invalid")] {
            invalid.scope = "unknown".into();
            assert_eq!(
                check(vec![scope(temp.path(), "write")], vec![invalid]).conflicts[0].reason,
                "scope_unresolved"
            );
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"untouched");
    }

    #[test]
    fn a_global_unknown_owner_blocks_disjoint_paths_and_request_overlap_is_an_issue() {
        let temp = tempfile::tempdir().unwrap();
        let mut global = claim(None, "exclusive");
        global.scope = "global".into();
        global.reason = "unknown_game_writes".into();
        let report = check(vec![scope(temp.path(), "write")], vec![global]);
        assert_eq!(report.conflicts[0].reason, "unknown_game_writes");
        assert!(report.conflicts[0].requested_path.is_none());
        let report = check(
            vec![scope(temp.path(), "write"), scope(temp.path(), "write")],
            vec![],
        );
        assert_eq!(report.scope_issues[0].code, "REQUEST_SCOPE_OVERLAP");
    }

    #[test]
    fn admission_rejects_unknown_access_even_without_active_owners() {
        let temp = tempfile::tempdir().unwrap();
        for access in ["", "readonly", "exclusive"] {
            let report = check(vec![scope(temp.path(), access)], vec![]);
            assert_eq!(
                report.ensure_admissible().unwrap_err().code,
                "RESOURCE_ACCESS_INVALID"
            );
        }
        assert_eq!(
            compare("i".into(), vec![], vec![]).unwrap_err().code,
            "RESOURCE_SCOPE_UNAVAILABLE"
        );
    }

    #[test]
    fn admission_enforces_read_write_conflicts_and_unknown_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        for (request, held, allowed) in [
            ("read", "read", true),
            ("write", "read", false),
            ("read", "write", false),
            ("write", "write", false),
        ] {
            let report = check(vec![scope(&a, request)], vec![claim(Some(&a), held)]);
            assert_eq!(report.ensure_admissible().is_ok(), allowed);
        }
        // Independent directories pass; an occupied directory remains protected.
        check(vec![scope(&a, "write")], vec![claim(Some(&b), "write")])
            .ensure_admissible()
            .unwrap();
        assert_eq!(
            check(vec![scope(&b, "read")], vec![claim(Some(&b), "write")])
                .ensure_admissible()
                .unwrap_err()
                .code,
            "TASK_CONFLICT"
        );
        let mut unknown = claim(None, "exclusive");
        unknown.scope = "global".into();
        unknown.owner_kind = "session".into();
        unknown.state = "interrupted_unknown".into();
        assert_eq!(
            check(vec![scope(&a, "write")], vec![unknown])
                .ensure_admissible()
                .unwrap_err()
                .code,
            "GAME_RUNNING_OR_UNKNOWN"
        );
    }

    #[test]
    fn excessive_claims_never_return_a_partial_clearance() {
        let error = compare(
            "instance".into(),
            vec![],
            vec![claim(None, "write"); 10_001],
        )
        .unwrap_err();
        assert_eq!(error.code, "RESOURCE_SCOPE_LIMIT");
    }
}
