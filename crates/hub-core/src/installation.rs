use crate::{
    Hub, HubError, Installation, Result,
    database::{id, now},
    paths,
};

impl Hub {
    pub async fn import_installation(&self, executable: &str, name: &str) -> Result<Installation> {
        let _guard = self.mutation.lock().await;
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
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO installation VALUES (?,?,?,?,?,?,?,?,?,?)")
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
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&installation.id)
            .bind("installation.import")
            .bind("succeeded")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(installation)
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
