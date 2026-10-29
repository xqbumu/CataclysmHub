//! User preferences are separate from workspace/backup identities and safety records.
use crate::{
    Hub, HubError, Result,
    database::{id, now},
    paths,
};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, OpenOptions};
use serde::Serialize;
use std::{collections::BTreeMap, io::Write};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct HubPreferences {
    #[ts(type = "number")]
    pub revision: i64,
    pub instance_profiles: BTreeMap<String, String>,
}
#[derive(Debug, Serialize, TS)]
pub struct PreferencesReset {
    pub backup_path: String,
    pub preferences: HubPreferences,
}
impl Hub {
    pub async fn preferences(&self) -> Result<HubPreferences> {
        let (revision, profiles): (i64, String) =
            sqlx::query_as("SELECT revision,instance_profiles FROM hub_preferences WHERE id=1")
                .fetch_one(&self.pool)
                .await
                .map_err(HubError::db)?;
        let instance_profiles = serde_json::from_str(&profiles).map_err(|_| {
            HubError::new("PREFERENCES_INVALID", "Hub 配置无法读取，请保留元数据。")
        })?;
        Ok(HubPreferences {
            revision,
            instance_profiles,
        })
    }
    pub async fn save_instance_profile(
        &self,
        instance_id: &str,
        profile: &str,
    ) -> Result<HubPreferences> {
        let _guard = self.mutation.lock().await;
        crate::instance_launch::validate_profile(profile)?;
        if !self
            .play_instances()
            .await?
            .iter()
            .any(|i| i.id == instance_id)
        {
            return Err(HubError::new("INSTANCE_NOT_FOUND", "游玩实例不存在。"));
        }
        let mut preferences = self.preferences().await?;
        preferences
            .instance_profiles
            .insert(instance_id.to_owned(), profile.to_owned());
        preferences.revision += 1;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("UPDATE hub_preferences SET revision=?,instance_profiles=? WHERE id=1")
            .bind(preferences.revision)
            .bind(
                serde_json::to_string(&preferences.instance_profiles)
                    .map_err(|_| HubError::new("PREFERENCES_INVALID", "配置编码失败。"))?,
            )
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(instance_id)
            .bind("preferences.instance_profile")
            .bind("succeeded")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(preferences)
    }
    pub async fn reset_preferences(
        &self,
        expected_revision: i64,
        confirmed: bool,
    ) -> Result<PreferencesReset> {
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请确认备份并重置 Hub 配置。",
            ));
        }
        let _guard = self.mutation.lock().await;
        let previous = self.preferences().await?;
        if previous.revision != expected_revision {
            return Err(HubError::new(
                "PREFERENCES_CHANGED",
                "配置已变化，请重新查看重置预览。",
            ));
        }
        let backup_error = |_| HubError::new("CONFIG_BACKUP_FAILED", "配置备份失败，未执行重置。");
        let root = Dir::open_ambient_dir(&self.data_dir, cap_std::ambient_authority())
            .map_err(backup_error)?;
        match root.create_dir("config-backups") {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(backup_error(e)),
        }
        let backups = root
            .open_dir_nofollow("config-backups")
            .map_err(backup_error)?;
        let name = format!("hub-preferences-{}.json", id());
        let snapshot = serde_json::json!({
            "schema_version": 1,
            "created_at": now(),
            "preferences": previous,
            "workspace_reference_only": self.dashboard().await?.workspace,
            "reset_scope": "instance_launch_preferences_only"
        });
        let bytes = serde_json::to_vec_pretty(&snapshot)
            .map_err(|_| HubError::new("CONFIG_BACKUP_FAILED", "配置备份编码失败，未执行重置。"))?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = backups.open_with(&name, &options).map_err(backup_error)?;
        file.write_all(&bytes).map_err(backup_error)?;
        file.sync_all().map_err(backup_error)?;
        #[cfg(unix)]
        {
            backups.into_std_file().sync_all().map_err(backup_error)?;
            root.into_std_file().sync_all().map_err(backup_error)?;
        }
        // Keep the successful backup even if the following metadata transaction fails.
        let backup_path = paths::text(&self.data_dir.join("config-backups").join(&name))?;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query(
            "UPDATE hub_preferences SET revision=revision+1,instance_profiles='{}' WHERE id=1",
        )
        .execute(&mut *tx)
        .await
        .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&name)
            .bind("preferences.reset")
            .bind("succeeded")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(PreferencesReset {
            backup_path,
            preferences: self.preferences().await?,
        })
    }
}
