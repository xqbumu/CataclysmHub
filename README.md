# Cataclysm Hub

跨平台、本地优先的 Cataclysm 游戏、资源、世界存档、Git 历史与独立备份管理器。

当前已提供 Tauri 2 + React 桌面开发预览：工作区配置、游戏导入与启动、进程监测、只读存档扫描、整个存档根目录的独立完整备份与回读校验，以及 SQLite 操作记录。桌面已接入新目录恢复预览、确认、任务、取消与审计，另提供无需数据库的独立恢复命令行工具；Git 历史与覆盖恢复仍在开发中。

## Taskfile 开发入口

安装 Task 3 后，在仓库执行 `task --list` 查看任务。原 npm / Cargo 命令仍可直接使用。

| 命令 | 用途 |
|---|---|
| `task deps` | `npm ci` 并获取锁定的 Rust 依赖 |
| `task dev` / `task dev:web` | 桌面开发 / 浏览器预览 |
| `task check` | 顺序执行前端、全特性 Rust 和 Python 检查 |
| `task check:frontend` / `task check:rust` / `task check:python` | 单独检查 |
| `task bindings` | 更新前端 DTO |
| `task build:frontend` | 前端生产构建 |
| `task build:debug` | macOS 普通 Debug 应用：`target/debug/bundle/macos/` |
| `task build:release` | macOS 普通 Release 应用：`target/release/bundle/macos/` |
| `task build:experimental` | macOS 并发候选 Debug 应用：`target/experimental/debug/bundle/macos/` |
| `task install:macos` | 先构建普通 Release，再安装到 `~/Applications` |
| `task install:macos:debug` | 先构建普通 Debug，再安装 |
| `task install:macos:experimental` | 显式构建并安装并发候选 Debug |

安装目录可通过 `task install:macos INSTALL_DIR="/Applications"` 指定；需要对该目录有写权限，任务不使用 sudo。每次安装都会先构建对应变体，不会把上次残留的实验包误作普通包。普通与实验构建仍共用前端输出目录，请勿并行执行构建或安装任务。

安装脚本检查应用标识和入口，发现任何名为 `cataclysm-hub` 的运行进程时拒绝替换；不会强杀、自动启动应用或迁移元数据。新包在目标目录内暂存，复制失败不动旧包；旧包以 `Cataclysm Hub.previous-<随机标识>.app` 保留，发布失败尝试恢复旧路径。安装期间请保持应用关闭；进程检查不提供对外部启动的原子锁，也不替代签名/公证认证。

实验构建仅启用固定 H/I QA 候选，不代表任意游戏版本可并发或真实验收已通过。需要隔离 QA 元数据时使用 Debug 构建及 `CATACLYSM_HUB_DATA_DIR`；Release 忽略该变量。实验包与普通包使用相同应用标识，安装实验包会替换同目录下普通包，旧包仍保留；可将 `INSTALL_DIR` 指向单独测试目录。

## 本地开发

需要 Node.js 24、npm 11、Rust 1.99.1 及对应平台的 Tauri 系统依赖。macOS 需要 Xcode；当前实际验证以本机 macOS Apple Silicon 为准。

```sh
npm ci
npm run desktop dev
```

仅预览界面使用 `npm run dev`；浏览器不能选择本地目录或启动程序，不包含模拟游戏数据。

```sh
npm run check                     # 格式、TypeScript、组件测试和前端构建
cargo test -p hub-core --locked    # 文件路径、数据库、真实合成进程集成测试
cargo clippy -p hub-core --all-targets --locked -- -D warnings
npm run bindings                  # 从 Rust DTO 生成 TypeScript 类型
npm run desktop -- build --debug --bundles app
```

macOS 打包命令生成 `target/debug/bundle/macos/Cataclysm Hub.app`。开发预览包不代表完成正式签名、公证或兼容认证。

应用元数据存放于 Tauri 的应用数据目录，不写入游戏安装或存档。调试构建可设置 `CATACLYSM_HUB_DATA_DIR` 隔离测试元数据；正式 release 构建忽略该变量。新工作区预填系统应用本地数据位置下的推荐 saves、backups、content 目录，确认保存时才创建；也可用本机选择器选择三个现有、相互独立的自定义目录。既有工作区不自动迁移。新游玩实例默认创建独立托管目录，也支持选择自定义空目录。之后通过本机文件选择器导入可信游戏程序。macOS 当前需要进入 `.app/Contents/MacOS` 选择实际可执行文件。

设置页提供“备份并重置 Hub 配置”：先保存旧配置 JSON，再重置每实例启动偏好与界面筛选；保留工作区路径、业务记录和游戏文件。模组与声音页先选安装版本，再选其关联实例进行读取和配置。目录约定、重置范围及验证证据见 [平台设置与实例操作](docs/design/platform-settings-instance-workflow.md)。

游戏版本和实际存档绑定尚未验证，启动不会自动追加存档参数。监测到直接进程退出也不会创建自动检查点或宣称存档稳定。

进入“世界与角色”后可扫描已配置的存档根目录。所有子目录目前都只视为候选，不解析或合并角色身份。扫描上限为 20,000 个条目、32 层目录；达到限制或遇到无法读取/无损表示的名称会显示不完整报告。链接只列出，不读取目标。扫描与管理器启动操作互斥，扫描结果不代表已冻结的数据快照。

元数据已升级至 schema 15。打开旧工作区前会在元数据目录创建并校验 `metadata-v{旧版本}-{uuid}.sqlite` 备份，再事务执行迁移。该文件是元数据保护副本，不是游戏存档备份。

### 获取游戏与资源

“游戏库 → 获取游戏”从 DDA、BN、TLG、EOD、TISH 的官方仓库读取近期发布及最新非预发布版本。首次启动自动尝试同步，之后点击“更新版本”更新；离线失败保留缓存，不自动重试。“模组与声音 → 下载资源”提供 CC-Sounds、Kenan DDA 历史包及 BN 固定提交包。下载支持进度、取消、校验和原始包删除；解包后仍需通过已有入口登记游戏或单个资源目录，再配置实例。详见 [上游目录与下载说明](docs/design/upstream-catalog-downloads.md)。

## 独立完整备份

进入“独立备份”，预览整个存档根目录的文件范围，关闭游戏并确认尽力快照风险后创建。应用先复制和哈希比对源文件，再写 TAR+Zstandard，回读校验通过后发布 `{backup_id}/archive.tar.zst` 与 `receipt.json`。源文件保持不变。所有新备份的一致性标识为 best_effort，文件完整性通过不代表游戏语义一致。

当前上限为 20,000 个条目、32 层目录、64 GiB 普通文件、8 MiB 清单。链接、特殊文件、Unix 硬链接或无法完整读取的范围会中止，不会静默丢弃文件。失败/取消可能保留带任务标识的 `.partial-{task_id}` 目录；它不会计为有效备份，也不会自动删除。

以下工具可在没有原数据库或 Git 仓库时独立校验备份目录，不执行恢复或解包写入：

```sh
cargo run -p hub-core --bin verify-backup -- /path/to/backup-id
```

`receipt.json` 丢失时，工具仍可依照归档内部清单逐文件校验，并返回 `receipt_present: false`。哈希不证明发布者身份。当前仅支持本应用生成的 v1 存档根目录格式；跨平台恢复、ACL/xattr 和游戏加载兼容性尚未验证。

### 备份名称与保留标记

备份卡的“编辑名称与保留锁”可设置名称、多行备注和本机保留锁，取消不保存。名称、备注和锁存入本机元数据，不修改归档内容、UUID 目录或校验事实；外置备份单独搬到另一台机器不会自动携带这些标注。编辑采用修订号检查，旧草稿不能覆盖新信息。

当前提供保留规则只读预览，可模拟最近份数及 UTC 日/周窗口，展示命名、锁定、恢复引用和历史校验保底等原因；不读取介质、不执行清理或删除。版本/存档环境关联尚未验证，规则外候选不等于允许删除。保留锁不限制操作系统文件操作；后续清理仍需后端检查命名/锁定、最后有效副本与介质情况。实现边界见 [备份标注设计](docs/design/backup-details-implementation.md)。

## 恢复到全新目录

桌面在“独立备份”选择某份归档的“恢复到新目录”，通过本机选择器选父目录。预览展示将新建的 restored-{uuid} 完整路径、容量、归档摘要和风险；关闭相关游戏并确认后才开始写入。任务支持阶段查询与取消，重启后中断任务不自动重放，成功也不会自动切换当前存档。

仅支持整个存档根目录 v1 归档。目标必须尚不存在，连已有空目录也不会写入。独立命令行执行：

```sh
cargo run -p hub-core --bin restore-backup -- /path/to/backup-id /path/to/new-recovery --confirm-new-directory
```

恢复文件位于 `new-recovery/data/`。工具先校验整个归档，再受控解包，最后重新读取目标文件核对路径集合、类型、大小与 SHA-256。成功返回 `state: verified` 并写入 `restore-verification.json`；该状态仅表示恢复字节与清单一致，不代表游戏可加载或兼容。

失败保留 `RESTORE_INCOMPLETE.json` 和可能的部分文件，不会自动清理或重放；不要把未完成目录作为存档使用。真实磁盘故障可能使标记写入也失败，必须结合命令退出状态判断。工具不切换当前存档，不覆盖既有目录，也不依赖原应用数据库。桌面使用同一恢复核心，任务与备份、扫描及游戏启动互斥。详细流程与限制见 [新目录恢复设计](docs/design/new-directory-restore-implementation.md)。

### 只读检查已恢复目录

桌面恢复任务下可直接选择“检查已恢复目录”，查看持久检查结果、时间、原标记及摘要；检查可取消，与其他工作区任务互斥。原恢复状态保持历史结论，新的检查失败会单独展示，不用旧成功记录掩盖变化。

中断后或日后需要确认恢复文件时，也可在关闭外部写入者后独立检查，不要求原应用数据库存在：

```sh
cargo run -p hub-core --bin verify-restored -- /path/to/backup-id /path/to/recovery-container
```

第二个参数是包含 data/ 的恢复容器。工具重新验证源归档，再逐项核对恢复载荷的路径、类型、大小和 SHA-256；不会写文件、修补标记、改判任务或再次恢复。输出中的 `verification.state=verified` 只代表本次观测的字节匹配，`incomplete_marker_present` 和 `historical_report_present` 分别报告原标记是否存在，不能据此宣称原任务成功或游戏兼容。文件变化、缺失、额外条目、类型不符或不可完整读取都会失败。

## 开发文档

- [开发文档总览](docs/README.md)：按角色阅读、文档责任和评审顺序。
- [原始 PRD 1.0.0](docs/requirements/PRD-1.0.0.md)：原文副本。
- [领域术语](CONTEXT.md)：统一世界、检查点、备份和验证的含义。
- [开发计划与工作包](docs/delivery/development-plan.md)：依赖、交付物和完成条件。
- [需求追踪矩阵](docs/requirements/traceability.md)：需求到设计、工作包和测试的对应关系。
- [当前实现与验证状态](docs/delivery/implementation-status.md)：已交付能力、设计差距和下一步。

首要验证目标为 macOS Apple Silicon、CDDA 0.I Ito、MSXotto+；此处表示验证目标，不表示已完成兼容性认证。
