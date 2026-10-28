//! Bind a configuration proposal to the existing, independently verified instance backup flow.
use crate::{BackupPlan, ContentConfigInput, ContentConfigPlan, Hub, HubError, Result};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentConfigProtectionPlan {
    pub configuration: ContentConfigPlan,
    pub backup: BackupPlan,
}
impl Hub {
    pub async fn prepare_content_config_protection(
        &self,
        instance_id: &str,
        input: ContentConfigInput,
    ) -> Result<ContentConfigProtectionPlan> {
        let configuration = self.preview_content_config(instance_id, input).await?;
        if configuration.files.is_empty() {
            return Err(HubError::new(
                "CONTENT_CONFIG_NO_CHANGES",
                "没有拟修改配置，无需为此草稿创建保护备份。",
            ));
        }
        let backup = self.prepare_instance_backup(instance_id).await?;
        // Both preparations have their own mutation gate. Compare their observations;
        // create_backup rechecks source scope and identity; pinned config hashes are checked while copying.
        if backup.play_instance_id.as_deref() != Some(instance_id)
            || backup.source_root != configuration.report.data_root
        {
            self.backup_plans.lock().await.remove(&backup.id);
            return Err(HubError::new(
                "CONTENT_CONFIG_CHANGED",
                "配置预览与备份清单不一致，请重新读取。",
            ));
        }
        let backup = self
            .bind_config_backup(&backup.id, &configuration.files)
            .await?;
        Ok(ContentConfigProtectionPlan {
            configuration,
            backup,
        })
    }
}
