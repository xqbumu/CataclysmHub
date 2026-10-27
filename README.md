# Cataclysm Hub

跨平台、本地优先的 Cataclysm 游戏、资源、世界存档、Git 历史与独立备份管理器。

当前已提供 Tauri 2 + React 桌面开发预览：工作区目录配置、本地游戏可执行文件导入、带确认的启动预览、直接子进程监测、只读存档目录扫描，以及 SQLite 操作记录。扫描支持世界候选、共享/未知文件、问题清单、取消与重启中断记录；Git 检查点与备份恢复尚在开发计划中。

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

元数据已升级至 schema 2。打开 schema 1 工作区前会在元数据目录创建并校验 `metadata-v1-{uuid}.sqlite` 备份，再事务执行迁移。该文件是元数据保护副本，不是游戏存档备份。

## 开发文档

- [开发文档总览](docs/README.md)：按角色阅读、文档责任和评审顺序。
- [原始 PRD 1.0.0](docs/requirements/PRD-1.0.0.md)：原文副本。
- [领域术语](CONTEXT.md)：统一世界、检查点、备份和验证的含义。
- [开发计划与工作包](docs/delivery/development-plan.md)：依赖、交付物和完成条件。
- [需求追踪矩阵](docs/requirements/traceability.md)：需求到设计、工作包和测试的对应关系。
- [当前实现与验证状态](docs/delivery/implementation-status.md)：已交付能力、设计差距和下一步。

首要验证目标为 macOS Apple Silicon、CDDA 0.I Ito、MSXotto+；此处表示验证目标，不表示已完成兼容性认证。
