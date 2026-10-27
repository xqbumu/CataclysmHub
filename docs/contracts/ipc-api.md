# 桌面命令、事件与错误契约

版本：0.1.0 · 日期：2026-10-27 · 状态：待评审 · 责任角色：前后端负责人。依据：PRD §16.4、APP-002～004、§19。

## 传输约定

这是进程内桌面 IPC 契约，不新增 HTTP 服务。协议版本为 1，所有请求包含 request_id；写操作包含 idempotency_key。后端先验证 schema、路径授权、资源引用、状态和锁，再调业务服务。前端只能使用显式注册命令；不得暴露通用 shell、任意文件写入或任意 SQL。

```typescript
type Id = string;
type ByteCount = string; // 非负十进制，避免 JS 精度丢失
type ApiResult<T> =
  | { ok: true; request_id: Id; data: T }
  | { ok: false; request_id: Id; error: {
      code: string; message_key: string; retryable: boolean;
      task_id?: Id; details: Record<string, unknown>;
    }};
type TaskAccepted = { task_id: Id; state: "queued" | "running" };
type PlanConfirmation = { plan_id: Id; plan_digest: string; confirmed: true };
```

示例仅描述契约，不是已生成客户端。读接口返回稳定分页 cursor；默认 page_size=50，上限 200（工程建议）。大清单、diff 分页或流式读取，禁止一次把整个存档内容送入 UI。

## 命令目录

| 命令 | 请求核心字段 | 返回 | 关键前置条件 |
|---|---|---|---|
| workspace.initialize | 用户选择的路径句柄 | workspace_id、路径校验 | 路径真实身份、不嵌套、权限 |
| installation.import | path_handle | TaskAccepted | 不下载；结果含 unknown 信息 |
| installation.list | cursor, filters | 安装页 | 本地查询 |
| game.launch_plan | installation_id, profile_id?, world_id?, args[] | plan | 参数、工作目录、存档范围可见 |
| game.launch | confirmation | TaskAccepted | 计划有效；不与恢复锁冲突 |
| world.scan | save_root_id | TaskAccepted | 只读发现，不自动确认身份 |
| world.list/update_metadata | filters / id, revision, metadata | 世界页 / 新 revision | 并发版本冲突返回错误 |
| checkpoint.plan/create | world_id, mode / confirmation | plan / TaskAccepted | 稳定检查、范围与空间；初始化预览 |
| checkpoint.list/diff | world_id,cursor / checkpoint_id,path,cursor | 历史页 / 差异页 | 原始 diff 仅本地展示 |
| repository.verify | repository_id | TaskAccepted | 只读对象与载荷检查 |
| backup.plan/create | scope,target_id / confirmation | plan / TaskAccepted | 源目标分离，独立于 Git |
| backup.list/verify | filter / backup_id | 备份页 / TaskAccepted | verify 重新读取磁盘 |
| restore.plan | source_kind, source_id, target_handle, mode | RestorePlan | 校验来源、差异和目标 |
| restore.execute | confirmation | TaskAccepted | 复检指纹、锁、保护备份 |
| retention.plan/execute | scope,policy / confirmation | 删除计划 / TaskAccepted | 保底、锁定、引用复检 |
| content.scan/plan/apply | root / action,package,profile / confirmation | task / plan / task | 冲突和依赖未知显式报告 |
| profile.export/import_plan/apply | profile_id / file_handle / confirmation | 文件句柄 / plan / task | 外发前移除本机路径、凭证 |
| migration.plan/execute | source,target,profile / confirmation | plan / task | 源只读、跨平台预检 |
| task.get/list/cancel | task_id / cursor / task_id | 快照 / 页 / cancel_requested | cancel 返回请求接收，不代表已停止 |
| diagnostics.plan/export | time_range / confirmation | 内容清单 / 文件句柄 | 本地预览脱敏后导出 |
| settings.get/update | keys / revision,values | 设置及 revision | 高风险策略变更有后端校验 |

## 恢复计划结构

RestorePlan 含 plan_id、plan_digest、expires_at、source_id/source_digest、scope、target_ref、target_fingerprint、diff_counts、分页差异引用、required_space、protection_required、warnings、blocking_errors、compatibility_evidence。计划摘要覆盖所有有意义字段，后端保存原始不可变计划；客户端传回的摘要不被当成权限证明。

确认从受信任本地 UI 产生，与当前应用会话和计划绑定；不得用 `confirmed=true` 绕过后端验证。建议计划有效期 10 分钟；执行前仍重新扫描目标。预览后新增文件或更换卷返回 PLAN_STALE，即使未到期。成功/失败均记录计划摘要和用户确认时间。

## 事件与恢复订阅

统一事件：`task.changed`、`world.changed`、`monitor.health_changed`、`backup.health_changed`。包含 protocol_version、event_id、task_id?、revision、occurred_at、payload。task.changed 含 state、phase、completed_bytes/total_bytes?、completed_items/total_items?、cancellable、warning_codes。

事件可能丢失、重复或乱序；前端只应用更高 revision，重连先 task.list/get 对账再订阅。百分比无法估计时显示阶段和已处理量，禁止伪造精确剩余时间。单任务事件节流建议 5 次/秒，状态转移立即发送。

## 错误字典

| code | 是否可自动重试 | 用户操作/语义 |
|---|---|---|
| PATH_INVALID / PATH_OVERLAP / PATH_UNSUPPORTED | 否 | 选择其他路径或查看冲突清单 |
| PERMISSION_DENIED / VOLUME_UNAVAILABLE | 否 | 恢复权限/连接后手动重试 |
| DISK_FULL | 否 | 释放空间；当前产物不发布 |
| GAME_RUNNING / SOURCE_CHANGED | 有界等待 | 关闭游戏或等待稳定 |
| BOUNDARY_UNKNOWN | 否 | 确认范围或选择根目录备份 |
| GIT_UNAVAILABLE / GIT_CORRUPT | 否 | 修复 Git；完整备份仍可用 |
| ARCHIVE_INVALID / HASH_MISMATCH / LIMIT_EXCEEDED | 否 | 停止恢复，选择有效来源 |
| PLAN_STALE / REVISION_CONFLICT | 否 | 重新预览并确认 |
| PROTECTION_FAILED | 否 | 中止覆盖；先修复保护条件 |
| RECOVERY_REQUIRED | 否 | 查看恢复向导，禁止自动重放 |
| SCHEMA_UNSUPPORTED / DB_MIGRATION_FAILED | 否 | 使用兼容版本或元数据恢复 |
| PARSER_UNSUPPORTED / SUMMARY_FAILED | 不阻断主任务 | 文件级摘要降级 |

details 只包含稳定机器字段和脱敏定位，不含原始存档、token 或完整个人路径。面向用户的文本由 message_key 本地化，代码不依赖文案字符串判断错误。
