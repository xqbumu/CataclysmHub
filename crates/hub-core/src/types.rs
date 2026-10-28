use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct HubError {
    pub code: &'static str,
    pub message: &'static str,
}
impl HubError {
    pub fn new(code: &'static str, message: &'static str) -> Self {
        Self { code, message }
    }
    pub(crate) fn db(_: impl std::fmt::Display) -> Self {
        Self::new(
            "DATABASE_ERROR",
            "无法读写本地元数据。原始游戏文件未被修改。",
        )
    }
}
impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for HubError {}
pub type Result<T> = std::result::Result<T, HubError>;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct Workspace {
    pub id: String,
    pub save_dir: String,
    pub backup_dir: String,
    pub content_dir: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceInput {
    pub save_dir: String,
    pub backup_dir: String,
    pub content_dir: String,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct Installation {
    pub id: String,
    pub name: String,
    pub executable: String,
    pub working_dir: String,
    pub sha256: String,
    pub version: Option<String>,
    pub branch: Option<String>,
    pub platform: String,
    pub arch: String,
    pub imported_at: String,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct Session {
    pub play_instance_id: Option<String>,
    pub launch_profile_id: Option<String>,
    pub instance_data_root: Option<String>,
    pub id: String,
    pub installation_id: String,
    pub state: String,
    #[ts(type = "number | null")]
    pub pid: Option<i64>,
    pub started_at: String,
    pub finished_at: Option<String>,
    #[ts(type = "number | null")]
    pub exit_code: Option<i64>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct OperationLog {
    pub id: String,
    pub task_id: String,
    pub action: String,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Serialize, TS)]
pub struct Dashboard {
    pub resource_claims: Vec<crate::ResourceClaim>,
    pub workspace: Option<Workspace>,
    pub installations: Vec<Installation>,
    pub play_instances: Vec<crate::PlayInstance>,
    pub sessions: Vec<Session>,
    pub operations: Vec<OperationLog>,
    pub scan_tasks: Vec<ScanTask>,
    pub backup_tasks: Vec<BackupTask>,
    pub backups: Vec<BackupRecord>,
    pub restore_tasks: Vec<RestoreTask>,
    pub inspection_tasks: Vec<InspectionTask>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct LaunchPlan {
    pub environment: std::collections::BTreeMap<String, String>,
    pub play_instance_id: Option<String>,
    pub launch_profile_id: Option<String>,
    pub instance_data_root: Option<String>,
    pub id: String,
    pub installation_id: String,
    pub executable: String,
    pub working_dir: String,
    pub args: Vec<String>,
    pub executable_sha256: String,
    pub save_dir: String,
    pub save_binding_verified: bool,
    pub expires_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct ScanTask {
    pub id: String,
    pub request_key: String,
    pub workspace_id: String,
    pub operation: String,
    pub state: String,
    pub phase: String,
    #[ts(type = "number")]
    pub processed_entries: i64,
    #[ts(type = "number")]
    pub revision: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ScanEntry {
    pub relative_path: String,
    pub file_type: String,
    pub size: String,
    pub candidate_root: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ScanIssue {
    pub code: String,
    pub relative_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WorldCandidate {
    pub id: String,
    pub relative_root: String,
    pub file_count: u32,
    pub entry_count: u32,
    pub total_bytes: String,
    pub identity_confidence: String,
    pub boundary_verified: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ScanReport {
    pub task_id: String,
    pub scanner_version: String,
    pub created_at: String,
    pub enumeration_complete: bool,
    pub visited_entries: u32,
    pub total_bytes: String,
    pub candidates: Vec<WorldCandidate>,
    pub entries: Vec<ScanEntry>,
    pub issues: Vec<ScanIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(deny_unknown_fields)]
pub struct BackupEntry {
    pub relative_path: String,
    pub file_type: String,
    pub size: String,
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mtime: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct BackupPlan {
    pub id: String,
    pub plan_digest: String,
    pub source_root: String,
    pub backup_dir: String,
    pub entry_count: u32,
    pub file_count: u32,
    pub total_bytes: String,
    pub required_free_bytes: String,
    pub consistency: String,
    pub expires_at: String,
    pub entries: Vec<BackupEntry>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct BackupTask {
    pub id: String,
    pub request_key: String,
    pub workspace_id: String,
    pub backup_id: String,
    pub operation: String,
    pub state: String,
    pub phase: String,
    #[ts(type = "number")]
    pub processed_entries: i64,
    #[ts(type = "number")]
    pub revision: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct BackupRecord {
    pub id: String,
    pub task_id: String,
    pub workspace_id: String,
    pub relative_dir: String,
    pub created_at: String,
    #[ts(type = "number")]
    pub entry_count: i64,
    #[ts(type = "number")]
    pub file_count: i64,
    pub total_bytes: String,
    pub consistency: String,
    pub verification: String,
    pub verified_at: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub display_name: String,
    pub notes: String,
    pub retention_locked: bool,
    #[ts(type = "number")]
    pub metadata_revision: i64,
    pub metadata_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RestorePlan {
    pub id: String,
    pub plan_digest: String,
    pub backup_id: String,
    pub destination: String,
    pub parent_dir: String,
    pub entry_count: u32,
    pub file_count: u32,
    pub total_bytes: String,
    pub consistency: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub expires_at: String,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct RestoreTask {
    pub id: String,
    pub request_key: String,
    pub workspace_id: String,
    pub backup_id: String,
    pub destination: String,
    pub state: String,
    pub phase: String,
    #[ts(type = "number")]
    pub revision: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct InspectionTask {
    pub id: String,
    pub request_key: String,
    pub restore_task_id: String,
    pub state: String,
    pub phase: String,
    #[ts(type = "number")]
    pub revision: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
    #[sqlx(skip)]
    pub result: Option<crate::RestoreInspection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BackupDetailsInput {
    pub backup_id: String,
    pub display_name: String,
    pub notes: String,
    pub retention_locked: bool,
    #[ts(type = "number")]
    pub expected_revision: i64,
}
