# 第一开发切片实现与验证状态

日期：2026-10-27 · 应用版本：0.1.0 开发预览 · 文档基线提交：`7d87f45`。

## 已实现能力

| 能力 | 代码入口 | 当前边界 |
|---|---|---|
| 桌面应用与工作台 | apps/desktop、src-tauri | Tauri 2、React、中文暗色界面；未实现能力明确禁用 |
| 工作区初始化 | hub-core/database.rs、paths.rs | 现有目录规范化、嵌套/同路径拒绝、写权限探测、持久化；不移动用户文件 |
| 本地安装导入 | hub-core/installation.rs | 用户通过本机选择器授权文件；SHA-256、平台、架构、路径记录；重复路径幂等；版本/分支 unknown |
| 启动计划与确认 | hub-core/process.rs | 参数数组、五分钟有效期、一次性计划、确认前后哈希校验、启动前审计意图 |
| 进程监测 | hub-core/process.rs | 直接子进程 wait/reap；退出记录 unverified；所有安装共享保守启动互斥 |
| 重启对账 | hub-core/database.rs | 遗留 starting/running 标记 interrupted_unknown；人工确认关闭后仍保持存档未知 |
| 元数据与审计 | hub-core/schema.sql | SQLite 初始 schema、事务、OS 单写实例锁、拒绝高版本数据库、固定字段本地日志 |
| IPC 类型 | hub-core/types.rs、export-bindings | Rust 作为 DTO 单一来源，生成的 TS 类型入库；CI 检测漂移 |
| 质量流水线 | .github/workflows/quality.yml | 前端检查、三平台核心测试、macOS 原生编译；远端 CI 尚未执行 |

## 工作包进度

WP-02 的工程骨架和基础 UI 已落地；WP-03 仅交付工作区持久化、启动意图及会话对账，通用后台任务、文件恢复 journal、范围锁服务和迁移备份未完成。WP-04 已实现手动选择程序的导入与启动，版本识别、.app 自动定位、平台进程树身份以及可靠存档绑定仍待适配器验证。因此 M1 仍为进行中，不将整个阶段标记完成。

M0 的真实 CDDA/资源样本、存档布局与兼容证据未取得。本次只使用自行生成的测试进程与临时数据；没有扫描、执行或修改玩家实际游戏与存档。

## IPC 过渡映射

当前使用 get_dashboard、pick_path、initialize_workspace、import_installation、create_launch_plan、launch_game、acknowledge_interrupted 七个命令。错误对象为 code/message；当前命令没有生产契约中的统一 request_id/envelope、持久 idempotency_key 和通用 TaskAccepted。初始化与导入通过事务/规范路径实现有限幂等，启动采用单次内存计划，应用重启后计划自动失效。

这套命令是第一切片适配层，不能宣称已完整实现 [IPC 设计](../contracts/ipc-api.md)。在加入世界写操作前，必须实现持久任务与范围锁并迁移至统一契约。当前没有任意 shell、SQL 或文件写入的 UI 命令。

## 验证证据

- Rust 集成测试 11 项通过：持久化、单实例、嵌套/符号链接路径、只读幂等导入、参数字面传递、明确确认、一次性计划、可执行文件变化、启动失败、重启未知状态、跨安装互斥。
- 前端 6 项测试通过：真实目录选择前置、未知备份状态、未实现动作禁用、启动预览与取消、明确确认、错误保留和参数格式。
- npm run check 通过：Prettier、TypeScript、Vitest 与 Vite production build。
- 浏览器在 1280×900 和 900×720 检查布局无横向溢出，概览/游戏库/工作区导航可用。
- macOS Apple Silicon 原生 debug `.app` 打包成功；通过实际原生选择器完成三目录配置、合成程序导入、启动预览、明确确认、运行/退出及四条审计记录验证。元数据和测试程序均置于隔离临时目录。
- cargo clippy -p hub-core --all-targets --locked -- -D warnings 通过。Windows/Linux、真实 CDDA 兼容和全部 AC 验收仍未执行。

## 下一开发切片

先取得目标版本的授权/合成布局 fixture，建立只读 WorldScanner、边界/未知文件报告、持久任务和路径范围锁；随后打通冻结副本→独立完整备份→空目录恢复与校验。覆盖恢复必须等保护备份、journal 和逐阶段故障注入通过后开放。

当前应用不提供自动备份或恢复，不改变用户现有备份方式。系统只监控直接进程；外部进程、子进程接管、哈希校验与实际执行间的本地竞态尚未完全解决，不能将本切片用作稳定性证明。
