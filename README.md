# Cataclysm Hub

跨平台、本地优先的 Cataclysm 游戏、资源、世界存档、Git 历史与独立备份管理器。

当前已提供 Tauri 2 + React 桌面开发预览：工作区配置、游戏导入与启动、进程监测、只读存档扫描、整个存档根目录的独立完整备份与回读校验，以及 SQLite 操作记录。另提供无需数据库的新目录恢复命令行工具；桌面恢复流程、Git 历史与覆盖恢复仍在开发中。

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

应用元数据存放于 Tauri 的应用数据目录，不写入游戏安装或存档。调试构建可设置 `CATACLYSM_HUB_DATA_DIR` 隔离测试元数据；正式 release 构建忽略该变量。初始化时选择三个现有、相互独立的目录，之后通过本机文件选择器导入可信游戏程序。macOS 当前需要进入 `.app/Contents/MacOS` 选择实际可执行文件。

游戏版本和实际存档绑定尚未验证，启动不会自动追加存档参数。监测到直接进程退出也不会创建自动检查点或宣称存档稳定。

进入“世界与角色”后可扫描已配置的存档根目录。所有子目录目前都只视为候选，不解析或合并角色身份。扫描上限为 20,000 个条目、32 层目录；达到限制或遇到无法读取/无损表示的名称会显示不完整报告。链接只列出，不读取目标。扫描与管理器启动操作互斥，扫描结果不代表已冻结的数据快照。

元数据已升级至 schema 3。打开旧工作区前会在元数据目录创建并校验 `metadata-v{旧版本}-{uuid}.sqlite` 备份，再事务执行迁移。该文件是元数据保护副本，不是游戏存档备份。

## 独立完整备份

进入“独立备份”，预览整个存档根目录的文件范围，关闭游戏并确认尽力快照风险后创建。应用先复制和哈希比对源文件，再写 TAR+Zstandard，回读校验通过后发布 `{backup_id}/archive.tar.zst` 与 `receipt.json`。源文件保持不变。所有新备份的一致性标识为 best_effort，文件完整性通过不代表游戏语义一致。

当前上限为 20,000 个条目、32 层目录、64 GiB 普通文件、8 MiB 清单。链接、特殊文件、Unix 硬链接或无法完整读取的范围会中止，不会静默丢弃文件。失败/取消可能保留带任务标识的 `.partial-{task_id}` 目录；它不会计为有效备份，也不会自动删除。

以下工具可在没有原数据库或 Git 仓库时独立校验备份目录，不执行恢复或解包写入：

```sh
cargo run -p hub-core --bin verify-backup -- /path/to/backup-id
```

`receipt.json` 丢失时，工具仍可依照归档内部清单逐文件校验，并返回 `receipt_present: false`。哈希不证明发布者身份。当前仅支持本应用生成的 v1 存档根目录格式；跨平台恢复、ACL/xattr 和游戏加载兼容性尚未验证。

## 恢复到全新目录（命令行）

仅支持整个存档根目录 v1 归档。目标必须尚不存在，连已有空目录也不会写入。先关闭可能使用相关路径的游戏，执行：

```sh
cargo run -p hub-core --bin restore-backup -- /path/to/backup-id /path/to/new-recovery --confirm-new-directory
```

恢复文件位于 `new-recovery/data/`。工具先校验整个归档，再受控解包，最后重新读取目标文件核对路径集合、类型、大小与 SHA-256。成功返回 `state: verified` 并写入 `restore-verification.json`；该状态仅表示恢复字节与清单一致，不代表游戏可加载或兼容。

失败保留 `RESTORE_INCOMPLETE.json` 和可能的部分文件，不会自动清理或重放；不要把未完成目录作为存档使用。真实磁盘故障可能使标记写入也失败，必须结合命令退出状态判断。工具不切换当前存档，不覆盖既有目录，也不依赖原应用数据库。桌面上的恢复预览、持久任务、取消与审计仍待接入。详细流程与限制见 [新目录恢复设计](docs/design/new-directory-restore-implementation.md)。

## 开发文档

- [开发文档总览](docs/README.md)：按角色阅读、文档责任和评审顺序。
- [原始 PRD 1.0.0](docs/requirements/PRD-1.0.0.md)：原文副本。
- [领域术语](CONTEXT.md)：统一世界、检查点、备份和验证的含义。
- [开发计划与工作包](docs/delivery/development-plan.md)：依赖、交付物和完成条件。
- [需求追踪矩阵](docs/requirements/traceability.md)：需求到设计、工作包和测试的对应关系。
- [当前实现与验证状态](docs/delivery/implementation-status.md)：已交付能力、设计差距和下一步。

首要验证目标为 macOS Apple Silicon、CDDA 0.I Ito、MSXotto+；此处表示验证目标，不表示已完成兼容性认证。
