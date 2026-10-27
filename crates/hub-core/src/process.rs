use crate::{
    Hub, HubError, LaunchPlan, Result, Session,
    database::{id, now},
    paths,
};
use std::{process::Stdio, sync::Arc};

impl Hub {
    pub(crate) async fn check_sessions(&self, _installation_id: &str) -> Result<()> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session WHERE state IN ('starting','running','interrupted_unknown')")
            .fetch_one(&self.pool).await.map_err(HubError::db)?;
        if count > 0 {
            return Err(HubError::new(
                "GAME_RUNNING_OR_UNKNOWN",
                "已有游戏运行或会话状态未知，请先确认游戏已关闭后再启动。",
            ));
        }
        let active: Vec<String> = sqlx::query_scalar("SELECT id FROM task WHERE state IN ('running','cancelling') UNION ALL SELECT id FROM backup_task WHERE state IN ('running','cancelling') UNION ALL SELECT id FROM restore_task WHERE state IN ('running','cancelling')")
            .fetch_all(&self.pool).await.map_err(HubError::db)?;
        let recovery = self.runtime_recovery.lock().await;
        if active.iter().any(|id| !recovery.contains_key(id)) {
            return Err(HubError::new(
                "TASK_CONFLICT",
                "工作区有活动扫描、备份或恢复任务，请等待完成或取消后再操作。",
            ));
        }
        Ok(())
    }
    pub async fn launch_plan(
        &self,
        installation_id: &str,
        args: Vec<String>,
    ) -> Result<LaunchPlan> {
        let _guard = self.mutation.lock().await;
        if args.len() > 64
            || args.iter().map(String::len).sum::<usize>() > 8192
            || args.iter().any(|a| a.contains('\0'))
        {
            return Err(HubError::new(
                "ARGS_INVALID",
                "启动参数过多、过长或含无效字符。",
            ));
        }
        self.check_sessions(installation_id).await?;
        let installation = self.installation(installation_id).await?;
        let workspace = self
            .dashboard()
            .await?
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let path = paths::canonical(&installation.executable, false)?;
        paths::executable(&path)?;
        if paths::text(&path)? != installation.executable
            || paths::digest(path).await? != installation.sha256
        {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "可执行文件自导入后发生变化，请核实来源后重新导入新的安装路径。",
            ));
        }
        let plan = LaunchPlan {
            id: id(),
            installation_id: installation.id,
            executable: installation.executable,
            working_dir: installation.working_dir,
            args,
            executable_sha256: installation.sha256,
            save_dir: workspace.save_dir,
            save_binding_verified: false,
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            warnings: vec![
                "游戏版本与分支尚未验证。".into(),
                "尚未验证此游戏实际使用的存档路径；选择工作区不会修改游戏配置。".into(),
                "当前只监测直接启动的进程，退出不代表存档稳定；不会创建自动检查点。".into(),
            ],
        };
        let mut plans = self.plans.lock().await;
        plans.retain(|_, p| p.expires_at > now());
        if plans.len() >= 100 {
            return Err(HubError::new("PLAN_LIMIT", "启动预览过多，请稍后重试。"));
        }
        plans.insert(plan.id.clone(), plan.clone());
        Ok(plan)
    }

    pub async fn launch(self: &Arc<Self>, plan_id: &str, confirmed: bool) -> Result<Session> {
        let _guard = self.mutation.lock().await;
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请检查启动预览并明确确认。",
            ));
        }
        let plan = self
            .plans
            .lock()
            .await
            .remove(plan_id)
            .ok_or(HubError::new("PLAN_STALE", "启动预览已失效，请重新生成。"))?;
        if plan.expires_at <= now() {
            return Err(HubError::new("PLAN_STALE", "启动预览已过期，请重新生成。"));
        }
        self.check_sessions(&plan.installation_id).await?;
        let path = paths::canonical(&plan.executable, false)?;
        if paths::text(&path)? != plan.executable
            || paths::digest(path.clone()).await? != plan.executable_sha256
        {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "预览后程序发生变化，启动已取消。",
            ));
        }
        paths::executable(&path)?;
        let session_id = id();
        let started = now();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query(
            "INSERT INTO session (id,installation_id,state,started_at) VALUES (?,?,'starting',?)",
        )
        .bind(&session_id)
        .bind(&plan.installation_id)
        .bind(&started)
        .execute(&mut *tx)
        .await
        .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&session_id)
            .bind("game.launch")
            .bind("starting")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        let child = tokio::process::Command::new(&path)
            .args(&plan.args)
            .current_dir(&plan.working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false)
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(_) => {
                self.finish_session(&session_id, "failed", None, Some("SPAWN_FAILED"))
                    .await?;
                return Err(HubError::new(
                    "SPAWN_FAILED",
                    "无法启动游戏。请检查权限、系统架构和游戏依赖。",
                ));
            }
        };
        let pid = child.id().map(i64::from);
        let recorded = sqlx::query("UPDATE session SET state='running',pid=? WHERE id=?")
            .bind(pid)
            .bind(&session_id)
            .execute(&self.pool)
            .await;
        // Always reap the child, including when recording its running state failed.
        let hub = Arc::clone(self);
        let watched_id = session_id.clone();
        tokio::spawn(async move {
            let outcome = child.wait().await;
            let (state, code, error) = match outcome {
                Ok(exit) => ("exited_unverified", exit.code().map(i64::from), None),
                Err(_) => ("interrupted_unknown", None, Some("PROCESS_WAIT_FAILED")),
            };
            // Preserve the earlier starting/running intent if persistence fails; restart reconciles it as unknown.
            if hub
                .finish_session(&watched_id, state, code, error)
                .await
                .is_err()
            {
                eprintln!("PROCESS_RESULT_PERSIST_FAILED task_id={watched_id}");
            }
        });
        recorded.map_err(|_| {
            HubError::new(
                "LAUNCH_STATUS_UNKNOWN",
                "程序可能已启动，但状态记录失败。请手动检查游戏，避免重复启动。",
            )
        })?;
        Ok(Session {
            id: session_id,
            installation_id: plan.installation_id,
            state: "running".into(),
            pid,
            started_at: started,
            finished_at: None,
            exit_code: None,
        })
    }

    async fn finish_session(
        &self,
        session_id: &str,
        state: &str,
        exit_code: Option<i64>,
        error: Option<&str>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("UPDATE session SET state=?,finished_at=?,exit_code=? WHERE id=?")
            .bind(state)
            .bind(now())
            .bind(exit_code)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(session_id)
            .bind("game.process_result")
            .bind(state)
            .bind(error)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
}
