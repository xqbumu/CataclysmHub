use crate::{
    BackupDetailsInput, BackupRecord, Hub, HubError, Result,
    database::{id, now},
};

fn validate(input: &mut BackupDetailsInput) -> Result<()> {
    let invalid_name_control = input.display_name.chars().any(char::is_control);
    input.display_name = input.display_name.trim().into();
    if input.expected_revision < 0
        || input.expected_revision > 9_007_199_254_740_991
        || input.display_name.chars().count() > 128
        || invalid_name_control
        || input.notes.chars().count() > 4000
        || input
            .notes
            .chars()
            .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
    {
        return Err(HubError::new(
            "BACKUP_DETAILS_INVALID",
            "备份信息或修订号无效。名称最多 128 个字符且不能含控制字符；备注最多 4000 个字符，可包含换行与制表符。",
        ));
    }
    Ok(())
}
impl Hub {
    /// Update local annotations only; archives, receipts, paths and verification facts are immutable here.
    pub async fn update_backup_details(
        &self,
        mut input: BackupDetailsInput,
    ) -> Result<BackupRecord> {
        validate(&mut input)?;
        let _gate = self.mutation.lock().await;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let current: BackupRecord = sqlx::query_as("SELECT * FROM backup_archive WHERE id=?")
            .bind(&input.backup_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new("BACKUP_NOT_FOUND", "备份记录不存在。"))?;
        // A lost response can be retried without advancing the revision or duplicating audit events.
        if current.display_name == input.display_name
            && current.notes == input.notes
            && current.retention_locked == input.retention_locked
        {
            tx.commit().await.map_err(HubError::db)?;
            return Ok(current);
        }
        if current.metadata_revision != input.expected_revision {
            return Err(HubError::new(
                "BACKUP_DETAILS_CONFLICT",
                "备份信息已更新，请载入最新信息后重新编辑。",
            ));
        }
        let revision = current
            .metadata_revision
            .checked_add(1)
            .filter(|revision| *revision <= 9_007_199_254_740_991)
            .ok_or(HubError::new(
                "BACKUP_DETAILS_REVISION_EXHAUSTED",
                "备份信息修订号已达上限，未修改记录。",
            ))?;
        let changed = now();
        let updated = sqlx::query("UPDATE backup_archive SET display_name=?,notes=?,retention_locked=?,metadata_revision=?,metadata_updated_at=? WHERE id=? AND metadata_revision=?")
            .bind(&input.display_name).bind(&input.notes).bind(input.retention_locked).bind(revision).bind(&changed).bind(&input.backup_id).bind(input.expected_revision).execute(&mut *tx).await.map_err(HubError::db)?;
        if updated.rows_affected() != 1 {
            return Err(HubError::new(
                "BACKUP_DETAILS_CONFLICT",
                "备份信息已更新，请载入最新信息后重新编辑。",
            ));
        }
        let details_changed =
            current.display_name != input.display_name || current.notes != input.notes;
        let lock_action = if current.retention_locked != input.retention_locked {
            Some(if input.retention_locked {
                "backup.retention_lock"
            } else {
                "backup.retention_unlock"
            })
        } else {
            None
        };
        // Keep annotation text out of audit logs; record only the changed operation and backup identity.
        for action in [
            details_changed.then_some("backup.details_update"),
            lock_action,
        ]
        .into_iter()
        .flatten()
        {
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(&input.backup_id)
                .bind(action)
                .bind("succeeded")
                .bind(Option::<String>::None)
                .bind(&changed)
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let record = sqlx::query_as("SELECT * FROM backup_archive WHERE id=?")
            .bind(&input.backup_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(record)
    }
}
