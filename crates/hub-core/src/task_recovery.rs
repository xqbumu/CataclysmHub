use crate::{
    Hub, HubError, Result,
    database::{id, now},
};
#[derive(Debug, Clone, Copy)]
pub(crate) enum TaskKind {
    Scan,
    Backup,
    Restore,
    Inspection,
}
impl TaskKind {
    fn table(self) -> &'static str {
        match self {
            Self::Scan => "task",
            Self::Backup => "backup_task",
            Self::Restore => "restore_task",
            Self::Inspection => "inspection_task",
        }
    }
    fn action(self) -> &'static str {
        match self {
            Self::Scan => "world.scan",
            Self::Backup => "backup.reconcile",
            Self::Restore => "backup.restore_new",
            Self::Inspection => "backup.inspect_restored",
        }
    }
}
#[derive(Debug, Clone)]
pub(crate) struct RuntimeFailure {
    pub kind: TaskKind,
    pub finished_at: String,
}
impl Hub {
    pub(crate) async fn mark_task_recovery(&self, kind: TaskKind, task_id: &str) {
        self.runtime_recovery.lock().await.insert(
            task_id.into(),
            RuntimeFailure {
                kind,
                finished_at: now(),
            },
        );
        if self.persist_task_recovery(kind, task_id).await.is_ok() {
            self.runtime_recovery.lock().await.remove(task_id);
        }
    }
    async fn persist_task_recovery(&self, kind: TaskKind, task_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let sql = format!(
            "UPDATE {} SET state='recovery_required',phase='recording_failed',error_code='TASK_RESULT_PERSIST_FAILED',finished_at=?,revision=revision+1 WHERE id=?",
            kind.table()
        );
        sqlx::query(&sql)
            .bind(now())
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(task_id)
            .bind(kind.action())
            .bind("recovery_required")
            .bind("TASK_RESULT_PERSIST_FAILED")
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
    /// Retry recording an already-ended worker, never replay its filesystem operation.
    pub async fn retry_task_recovery(&self, task_id: &str) -> Result<()> {
        let _gate = self.mutation.lock().await;
        let failure = self.runtime_recovery.lock().await.get(task_id).cloned();
        if let Some(failure) = failure {
            self.persist_task_recovery(failure.kind, task_id).await?;
            self.runtime_recovery.lock().await.remove(task_id);
        }
        Ok(())
    }
    pub(crate) async fn overlay_scan(&self, task: &mut crate::ScanTask) {
        if let Some(failure) = self.runtime_recovery.lock().await.get(&task.id) {
            task.state = "recovery_required".into();
            task.phase = "recording_failed".into();
            task.error_code = Some("TASK_RESULT_PERSIST_FAILED".into());
            task.finished_at = Some(failure.finished_at.clone());
            task.revision += 1;
        }
    }
    pub(crate) async fn overlay_backup(&self, task: &mut crate::BackupTask) {
        if let Some(failure) = self.runtime_recovery.lock().await.get(&task.id) {
            task.state = "recovery_required".into();
            task.phase = "recording_failed".into();
            task.error_code = Some("TASK_RESULT_PERSIST_FAILED".into());
            task.finished_at = Some(failure.finished_at.clone());
            task.revision += 1;
        }
    }
    pub(crate) async fn overlay_restore(&self, task: &mut crate::RestoreTask) {
        if let Some(failure) = self.runtime_recovery.lock().await.get(&task.id) {
            task.state = "recovery_required".into();
            task.phase = "recording_failed".into();
            task.error_code = Some("TASK_RESULT_PERSIST_FAILED".into());
            task.finished_at = Some(failure.finished_at.clone());
            task.revision += 1;
        }
    }
    pub(crate) async fn overlay_inspection(&self, task: &mut crate::InspectionTask) {
        if let Some(failure) = self.runtime_recovery.lock().await.get(&task.id) {
            task.state = "recovery_required".into();
            task.phase = "recording_failed".into();
            task.error_code = Some("TASK_RESULT_PERSIST_FAILED".into());
            task.finished_at = Some(failure.finished_at.clone());
            task.revision += 1;
            task.result = None;
        }
    }
}
