# 平台目录约定与仅配置重置调研

日期：2026-10-29。范围：macOS、Windows、Linux 桌面；Tauri 2 路径 API；当前仓库默认工作区与配置重置设计。以下明确区分官方事实、仓库现状与实现建议，不将建议当作已实现能力。

## 结论

长期游戏数据、备份和本地资源属于持久数据，不应放入缓存或临时目录。应通过系统路径 API 解析用户位置，以应用命名空间管理默认目录，并允许显式选择外部位置。重置配置应保留安装、实例、归档登记及恢复保护状态；已有工作区路径不是可以随意覆盖的普通偏好。

## 官方依据

### macOS

Apple 将应用自动管理的数据放在用户 Library 下的 Application Support；应用子目录按 bundle identifier 或公司命名。Caches 仅适合可以重建的内容，Preferences 应通过相应偏好 API 使用，不能把任意文件堆进去。Documents/Desktop 不应用于应用自动管理的数据。[Apple File System Programming Guide — macOS Library](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html#//apple_ref/doc/uid/TP40010672-CH2-SW6)

非沙箱 macOS 中，Application Support 和 Caches 通常分别解析到 `~/Library/Application Support` 与 `~/Library/Caches`；临时与缓存内容可能被系统清理，不能作为唯一持久副本。[Apple — Using the file system effectively](https://developer.apple.com/documentation/foundation/using-the-file-system-effectively)

因此 Hub 托管实例、资源及备份可在 Application Support 下使用应用命名空间；备份放在同一磁盘仍不能抵御该磁盘损坏，外置备份位置应继续允许选择。后一条是本项目的备份产品建议，不是 Apple 为 Hub 指定的目录。

### Windows

Win32 Known Folders 明确区分 `FOLDERID_LocalAppData`（默认 `%USERPROFILE%\AppData\Local`）、`FOLDERID_RoamingAppData` 及 `FOLDERID_SavedGames`（默认 `%USERPROFILE%\Saved Games`）。路径应由系统解析，不能将默认展开值当成用户机器上的实际位置。[Microsoft — KNOWNFOLDERID](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)

微软将设备特定、不适合漫游及体积大的数据归为 local app data；小型可跨设备偏好与机器绑定的路径状态应区别处理。该文同时包含 packaged/UWP 存储 API，不能把其中的容器清理行为直接套用于本仓库普通桌面安装。[Microsoft — Store and retrieve settings and other app data](https://learn.microsoft.com/en-us/windows/apps/design/app-settings/store-and-retrieve-app-data)

对 Hub：完整安装副本、实例、资源包和备份优先用 LocalAppData。Saved Games 可作为用户主动选择的位置，但不能将不同游戏版本的完整实例根全都塞入一个共享 save 目录。包含绝对本机路径的设置也不应默认作为可漫游偏好。

### Linux

XDG Base Directory 0.8 将持久用户数据、配置、状态、缓存区分为 `XDG_DATA_HOME`、`XDG_CONFIG_HOME`、`XDG_STATE_HOME`、`XDG_CACHE_HOME`。未设置或为空时分别使用 `~/.local/share`、`~/.config`、`~/.local/state`、`~/.cache`；这些变量若给出相对路径应忽略。日志／最近使用历史属于 state；临时运行目录不是持久存档目录。[freedesktop.org — XDG Base Directory Specification](https://specifications.freedesktop.org/basedir/latest/)

对 Hub：实例、备份、资源放 data 命名空间，真正的独立偏好放 config，下载缓存放 cache。不要自行固定成 `~/CataclysmHub` 或在当前工作目录创建所有内容。

### Tauri 2 的能力

Tauri 提供 `appConfigDir`、`appDataDir`、`appLocalDataDir`、`appCacheDir`、`appLogDir`。这些路径包含 bundle identifier，也允许应用目录覆盖配置；应在运行时解析，不应依赖开发机绝对路径。[Tauri — path API](https://v2.tauri.app/reference/javascript/api/namespacepath/)

本机锁定的 Tauri 2.12.5 Rust 实现同样提供 `app.path().app_config_dir()`、`app_data_dir()`、`app_local_data_dir()` 等方法。`app_data_dir()` 与 `app_local_data_dir()` 在 macOS/Linux 通常相同，在 Windows 分别对应 roaming/local。实际修改应以锁定依赖源码和目标平台测试为准；官网最新页面不代表本项目已经具备所有最新功能。

## 当前仓库约束

通过本地源码核查：

- `apps/desktop/src-tauri/src/lib.rs` 启动时使用 `app.path().app_data_dir()` 打开 Hub；仅 Debug 允许 `CATACLYSM_HUB_DATA_DIR` 覆盖。不能为本轮目录优化悄悄换元数据库位置，否则用户会看到空工作区，旧记录仍留在原位置。
- `crates/hub-core/src/database.rs::initialize` 要求存档、备份、资源和 `Hub.data_dir` 两两分离且不嵌套；选择路径必须已经存在。
- 因而直接推荐 `appLocalDataDir()/saves`、`/backups`、`/content` 在 macOS/Linux 会落到当前元数据根内，违反已有保护。
- `workspace` 保存三项真实目录；任务与归档以 `workspace_id` 关联。当前 dashboard 读取 `SELECT * FROM workspace LIMIT 1`，没有独立 active-workspace 设置。
- 桌面 `initialize_workspace` 目前只接受本机选择器授权过的路径。系统默认目录需要专门、受限的后端入口授权，不能通过给前端任意路径放行来实现。

这些是仓库现状，不是操作系统规范。

## 最小可落地方案

### 1. 新工作区默认目录

保留已有元数据位置及用户选择的目录。新工作区默认以系统 local-data 父目录下的独立应用工作区命名空间组织，例如：

| 平台 | 建议托管根（示意，须运行时解析） |
|---|---|
| macOS | `~/Library/Application Support/com.cataclysmhub.desktop.workspace/` |
| Windows | `%LOCALAPPDATA%\com.cataclysmhub.desktop.workspace\` |
| Linux | `$XDG_DATA_HOME/com.cataclysmhub.desktop.workspace/`，使用规范回退 |

根下采用互为同级的 `saves/`、`backups/`、`content/`、`instances/`，使前三者不相互嵌套且与旧元数据根分离。`.workspace` 是为兼容现有元数据边界选择的项目命名，不是系统强制后缀。通过本机 local-data API 派生此命名空间，不能简单假定 `.parent()` 永远符合目录覆盖策略；若允许自定义 Tauri 目录覆盖，需要显式验证最终物理路径。

默认路径只是建议。点击“使用系统默认目录”时由后端创建缺失的专用目录、核验权限和物理重叠，再初始化；不要启动即迁移已有玩家文件。Debug 隔离模式下也应派生隔离的工作区建议，避免测试重置触及真实用户位置。

用户导入现有实例仍保留原路径。实例根应使用 ID 子目录或用户选择的独立目录，不能仅用可能重复的世界名。缓存与日志可以后续分别使用 cache/log API；本轮不必引入新的大规模目录迁移框架。

### 2. 配置重置

“只重置配置”必须先明确配置数据边界。以下是推荐的最小契约：

1. 独立定义 Hub 设置对象（例如默认新工作区路径偏好、界面及启动方案偏好）；不要把整个 SQLite 当作可删除配置。
2. 后端持有 mutation 门，拒绝活动或未解除的恢复状态下改变有关路径行为。
3. 先将当前设置原字节或带版本的完整设置快照写入不可覆盖的新备份文件，完成写入及校验后才发布默认设置。备份失败则不重置。
4. 返回备份位置与重置范围；保留设置备份，不把它放在 reset 会清空的 cache/temp 中。
5. 不删除或改写安装、实例、会话、备份、审计、配置变更恢复记录，也不改游戏自己的 options/mods 文件。
6. 现有工作区的目录绑定保留。若产品要求将“既有工作区三路径”也恢复到默认，必须另行实现明确的工作区切换/迁移：旧 workspace ID 及路径继续保留，历史归档按旧路径解析。不能直接 UPDATE 三列后把旧归档定位到新备份目录。

以上重置步骤属于根据仓库安全边界提出的实现建议，系统官方目录文档并不规定 Hub 的重置事务。

## 验证要求

- 新默认目录：平台 API 解析结果、创建与重试幂等、无选择器路径绕过、目录不可写、路径别名/重叠拒绝、Debug 隔离。
- 重置：备份失败不变更、备份内容与原设置一致、重复重置不覆盖旧备份、重启默认值生效、游戏/存档/资源原字节不变、历史归档仍按原工作区可定位。
- 本机 macOS 实测不能宣称 Windows/Linux 路径和权限验收通过；后两者需对应平台测试或明确标记未验证。
