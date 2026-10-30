use crate::{
    Hub, HubError, LaunchPlan, Result, Session,
    database::{id, now},
    paths,
};
use std::{process::Stdio, sync::Arc};

pub(crate) struct PreparedLaunch {
    pub plan: LaunchPlan,
    pub candidate: Option<Arc<crate::candidate_launch::CandidateBinding>>,
    pub instance_identity: Option<Arc<(same_file::Handle, same_file::Handle, same_file::Handle)>>,
}

impl Hub {
    pub async fn launch_plan(
        &self,
        installation_id: &str,
        args: Vec<String>,
    ) -> Result<LaunchPlan> {
        let _guard = self.mutation.lock().await;
        self.launch_plan_inner(installation_id, args, None).await
    }
    // Callers hold mutation. Only a validated candidate may pass scoped_instance.
    pub(crate) async fn launch_plan_inner(
        &self,
        installation_id: &str,
        args: Vec<String>,
        scoped_instance: Option<&str>,
    ) -> Result<LaunchPlan> {
        if args.len() > 64
            || args.iter().map(String::len).sum::<usize>() > 8192
            || args.iter().any(|a| a.contains('\0'))
        {
            return Err(HubError::new(
                "ARGS_INVALID",
                "启动参数过多、过长或含无效字符。",
            ));
        }
        if let Some(instance) = scoped_instance {
            self.check_instance_resources(instance).await?;
        } else {
            self.check_resource_claims().await?;
        }
        let installation = self.installation(installation_id).await?;
        let workspace = self
            .dashboard()
            .await?
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let path = paths::canonical(&installation.executable, false)?;
        paths::executable(&path)?;
        crate::application_entry::validate_bundle_program(&path)?;
        if paths::text(&path)? != installation.executable
            || paths::digest(path).await? != installation.sha256
        {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "可执行文件自导入后发生变化，请核实来源后重新导入新的安装路径。",
            ));
        }
        let plan = LaunchPlan {
            environment: Default::default(),
            play_instance_id: None,
            launch_profile_id: None,
            instance_data_root: None,
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
        plans.retain(|_, p| p.plan.expires_at > now());
        if plans.len() >= 100 {
            return Err(HubError::new("PLAN_LIMIT", "启动预览过多，请稍后重试。"));
        }
        plans.insert(
            plan.id.clone(),
            PreparedLaunch {
                plan: plan.clone(),
                instance_identity: None,
                candidate: None,
            },
        );
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
        let prepared = self
            .plans
            .lock()
            .await
            .remove(plan_id)
            .ok_or(HubError::new("PLAN_STALE", "启动预览已失效，请重新生成。"))?;
        let plan = prepared.plan;
        if plan.expires_at <= now() {
            return Err(HubError::new("PLAN_STALE", "启动预览已过期，请重新生成。"));
        }
        if let Some(binding) = &prepared.candidate {
            let current = crate::candidate_launch::prepare(
                plan.launch_profile_id.as_deref().unwrap_or(""),
                &plan.executable,
                &plan.working_dir,
                plan.instance_data_root.as_deref().unwrap_or(""),
            )
            .await?
            .ok_or(HubError::new("PLAN_STALE", "并发候选预览已失效。"))?;
            if !binding.matches(&current) {
                return Err(HubError::new(
                    "PLAN_STALE",
                    "并发候选程序、静态资源或目录身份已变化。",
                ));
            }
        } else {
            self.check_resource_claims().await?;
        }
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
        crate::application_entry::validate_bundle_program(&path)?;
        if let Some(ref identity) = prepared.instance_identity {
            self.revalidate_instance_launch(&plan, identity).await?;
        } else if plan.play_instance_id.is_some() {
            return Err(HubError::new(
                "PLAN_STALE",
                "实例预览缺少目录身份，请重新生成。",
            ));
        }
        if let Some(instance_id) = &plan.play_instance_id {
            self.check_instance_resources(instance_id).await?;
        }
        let session_id = id();
        let started = now();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query(
            "INSERT INTO session (id,installation_id,state,started_at,play_instance_id,launch_profile_id,instance_data_root) VALUES (?,?,'starting',?,?,?,?)",
        )
        .bind(&session_id)
        .bind(&plan.installation_id)
        .bind(&started)
        .bind(&plan.play_instance_id)
        .bind(&plan.launch_profile_id)
        .bind(&plan.instance_data_root)
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
        if let Some(ref identity) = prepared.instance_identity
            && let Err(error) =
                crate::instance_launch::prepare_instance_directories(&plan, Arc::clone(identity))
                    .await
        {
            self.finish_session(&session_id, "failed", None, Some(error.code))
                .await?;
            return Err(error);
        }
        let mut command = tokio::process::Command::new(&path);
        if prepared.candidate.is_some() {
            // Do not inherit loader injection or shared HOME overrides into a
            // fixed candidate. All candidate environment is shown in its plan.
            command.env_clear();
        }
        let child = command
            .args(&plan.args)
            .envs(&plan.environment)
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
        let candidate = prepared.candidate.is_some();
        if recorded.is_ok()
            && let Some(binding) = prepared.candidate
        {
            self.candidate_sessions
                .lock()
                .await
                .insert(session_id.clone(), binding);
        }
        // Always reap the child, including when recording its running state failed.
        let hub = Arc::clone(self);
        let watched_id = session_id.clone();
        tokio::spawn(async move {
            let outcome = child.wait().await;
            let (state, code, error) = match outcome {
                Ok(exit) if candidate && !exit.success() => (
                    "interrupted_unknown",
                    exit.code().map(i64::from),
                    Some("CANDIDATE_EXIT_UNKNOWN"),
                ),
                // A signal has no numeric exit code. Preserve that fact in the
                // operation log; SIGKILL alone does not prove Gatekeeper rejection.
                Ok(exit) if exit.code().is_none() => (
                    "exited_unverified",
                    None,
                    Some("PROCESS_TERMINATED_BY_SIGNAL"),
                ),
                Ok(exit) => ("exited_unverified", exit.code().map(i64::from), None),
                Err(_) => ("interrupted_unknown", None, Some("PROCESS_WAIT_FAILED")),
            };
            // Revoke immediately, including while another launch owns the gate.
            // If recording termination fails, the surviving intent is global.
            hub.candidate_sessions.lock().await.remove(&watched_id);
            let _gate = hub.mutation.lock().await;
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
            play_instance_id: plan.play_instance_id,
            launch_profile_id: plan.launch_profile_id,
            instance_data_root: plan.instance_data_root,
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
