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
    pub workspace: Option<Workspace>,
    pub installations: Vec<Installation>,
    pub sessions: Vec<Session>,
    pub operations: Vec<OperationLog>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct LaunchPlan {
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
