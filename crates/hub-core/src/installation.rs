use crate::{
    Hub, HubError, Installation, Result,
    database::{id, now},
    paths,
};

impl Hub {
    pub async fn import_installation(&self, executable: &str, name: &str) -> Result<Installation> {
        let _guard = self.mutation.lock().await;
        self.check_resource_claims().await?;
        let installation = self.prepare_installation(executable, name).await?;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        Self::write_installation(&mut tx, &installation).await?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(installation)
    }
    pub(crate) async fn prepare_installation(
        &self,
        executable: &str,
        name: &str,
    ) -> Result<Installation> {
        if self.dashboard().await?.workspace.is_none() {
            return Err(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
            return Err(HubError::new(
                "NAME_INVALID",
                "安装名称须为 1～100 个可显示字符。",
            ));
        }
        let path = paths::canonical(executable, false)?;
        paths::executable(&path)?;
        let canonical = paths::text(&path)?;
        if let Some(existing) =
            sqlx::query_as::<_, Installation>("SELECT * FROM installation WHERE executable=?")
                .bind(&canonical)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?
        {
            return Ok(existing);
        }
        let working_dir = paths::text(
            path.parent()
                .ok_or(HubError::new("PATH_INVALID", "程序缺少有效工作目录。"))?,
        )?;
        let roots = self.play_instances().await?;
        let candidate_dir = paths::installation_scope(&working_dir);
        tokio::task::spawn_blocking(move || {
            for instance in roots {
                let root = paths::canonical(&instance.data_root, true)?;
                if crate::play_instances::overlaps(&candidate_dir, &root)? {
                    return Err(HubError::new(
                        "INSTANCE_SCOPE_OVERLAP",
                        "游戏安装目录不能与已登记实例目录重叠。",
                    ));
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| HubError::new("INSTANCE_CHECK_FAILED", "实例目录检查未完成。"))??;
        let installation = Installation {
            id: id(),
            name: name.to_owned(),
            executable: canonical,
            working_dir,
            sha256: paths::digest(path).await?,
            version: None,
            branch: None,
            platform: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            imported_at: now(),
        };
        Ok(installation)
    }
    pub(crate) async fn write_installation(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        installation: &Installation,
    ) -> Result<()> {
        let inserted = sqlx::query("INSERT INTO installation VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT(executable) DO NOTHING")
            .bind(&installation.id)
            .bind(&installation.name)
            .bind(&installation.executable)
            .bind(&installation.working_dir)
            .bind(&installation.sha256)
            .bind(&installation.version)
            .bind(&installation.branch)
            .bind(&installation.platform)
            .bind(&installation.arch)
            .bind(&installation.imported_at)
            .execute(&mut **tx)
            .await
            .map_err(HubError::db)?.rows_affected();
        if inserted == 0 {
            let existing: String =
                sqlx::query_scalar("SELECT id FROM installation WHERE executable=?")
                    .bind(&installation.executable)
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(HubError::db)?;
            if existing != installation.id {
                return Err(HubError::new(
                    "INSTALLATION_CHANGED",
                    "安装记录已变化，请重新检查。",
                ));
            }
            return Ok(());
        }
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&installation.id)
            .bind("installation.import")
            .bind("succeeded")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut **tx)
            .await
            .map_err(HubError::db)?;
        Ok(())
    }
    pub(crate) async fn installation(&self, installation_id: &str) -> Result<Installation> {
        sqlx::query_as("SELECT * FROM installation WHERE id=?")
            .bind(installation_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new(
                "INSTALLATION_NOT_FOUND",
                "安装记录不存在，请刷新游戏库。",
            ))
    }
}
