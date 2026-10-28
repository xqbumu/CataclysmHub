# Cataclysm Hub 开发文档总览

文档包版本：0.1.0 · 编制日期：2026-10-27 · 状态：待评审设计基线。

本套文档将 PRD 转化为可评审、可拆分、可验证的开发依据，覆盖需求治理、架构与详细设计、接口和格式、测试安全、开发排期、发布及运维。仓库已开始实现第一条桌面切片，具体完成范围和验证见 [实现状态](delivery/implementation-status.md)；PRD 全量测试、性能指标和兼容性仍为待执行计划。

## 来源与版本

原始输入为 `Cataclysm_Hub_Enterprise_PRD_1.0.0.md`，原文保存于 [需求基线](requirements/PRD-1.0.0.md)。SHA-256：`bbf31d3fc3b666a05d113fd66805163f121342cc4a4eca66e900c6a9a000e46d`。派生文档不改写原文；新增选择明确为工程建议或 proposed ADR。

首要验证环境：macOS Apple Silicon、CDDA 0.I Ito、MSXotto+。当前均为目标环境，真实格式、资源修订和兼容性待样本验证。

## 文档地图

| 文档 | 用途 | 主要责任角色 |
|---|---|---|
| [范围基线与治理](requirements/scope-and-governance.md) | 需求强度、范围冲突、评审角色、Ready/Done 和变更 | 产品/技术/QA |
| [需求追踪矩阵](requirements/traceability.md) | 84 个原始编号及 16 个派生条目的设计/工作包/测试入口 | QA |
| [领域术语](../CONTEXT.md) | 统一世界、检查点、备份、一致性与兼容性用语 | 全体 |
| [系统架构](architecture/system-design.md) | 模块所有权、依赖、路径布局、并发和降级 | 技术 |
| [一致性与任务](design/consistency-and-tasks.md) | 状态维度、冻结副本、取消、幂等和重启对账 | 核心服务 |
| [备份与恢复](design/backup-and-restore.md) | 清单、归档、保护、受控切换、保留与独立恢复 | 备份 |
| [Git 与摘要](design/checkpoint-and-summary.md) | 仓库隔离、原字节、证据、降级和 AI 边界 | 历史/摘要 |
| [数据模型](design/data-model.md) | 实体、关系、索引、不变量、事务和 schema 迁移 | 持久化 |
| [游戏资源与迁移](design/game-content-and-migration.md) | 适配器、启动、资源组合、档案和非破坏迁移 | 游戏/资源 |
| [只读候选扫描实现](design/read-only-world-inventory.md) | 当前扫描语义、持久任务、取消、限制和 schema 2 迁移 | 世界/核心服务 |
| [并发游玩补充](requirements/multi-version-concurrent-play.md) | USR-001，多版本同时游玩的隔离、范围锁与验收 | 游戏/核心服务 |
| [版本与存档影响](design/version-save-impact-scope.md) | 多版本身份、存档关联、升级影响及清理/恢复前置条件 | 游戏/备份 |
| [保留预览实现](design/retention-preview-implementation.md) | 只读规则并集、完整范围、UTC 窗口与不授权删除边界 | 备份/核心服务 |
| [备份标注实现](design/backup-details-implementation.md) | 本机命名/备注/保留锁、乐观修订与 schema 6 | 备份/核心服务 |
| [新目录恢复实现](design/new-directory-restore-implementation.md) | 桌面计划/任务、独立 CLI、拒绝覆盖、回读证明与 schema 4 | 备份/核心服务 |
| [完整备份实现](design/full-backup-implementation.md) | 范围确认、冻结复制、独立归档、取消、校验工具与 schema 3 | 备份/核心服务 |
| [交互规格](design/ux-specification.md) | 页面、空态、错误、危险确认、可访问性和本地化 | 设计/前端 |
| [IPC 契约](contracts/ipc-api.md) | 命令、DTO、计划确认、事件、分页和错误码 | 前后端 |
| [文件格式契约](contracts/file-formats.md) | 5 个 JSON Schema、合成正例和兼容演进 | 备份/档案 |
| [威胁模型](security/threat-model.md) | 信任边界、安全控制、隐私和供应链 | 安全 |
| [保留预览验证](quality/retention-preview-verification.md) | 完整范围、规则并集、只读证据与未授权删除 | QA/备份 |
| [备份标注验证](quality/backup-details-verification.md) | 标注与归档分离、事务回滚、原生界面与迁移 | QA/桌面 |
| [桌面检查验证](quality/desktop-inspection-verification.md) | 持久检查、原任务/字节分离、原生正反例与 schema 5 | QA/桌面 |
| [恢复强杀矩阵](quality/restore-crash-matrix.md) | 六个真实子进程终止边界、重启对账与只读检查证据 | QA/核心服务 |
| [测试与验收](quality/test-strategy.md) | 数据集、核心用例、故障注入、AC 和缺陷门禁 | QA |
| [性能与兼容](quality/performance-and-compatibility.md) | 性能预算、平台与游戏资源验证矩阵 | QA/平台 |
| [文档交付校验](quality/document-review.md) | 来源一致性、追踪覆盖、链接与 Schema 检查结果 | QA |
| [开发计划](delivery/development-plan.md) | M0～M6、18 个工作包、依赖和可演示切片 | 产品/技术 |
| [工程规范](delivery/engineering-standards.md) | 工程环境、编码、评审和完成要求 | 技术 |
| [CI/CD 与发布](delivery/release-and-cicd.md) | 流水线、签名、供应链、升级回退和发行证据 | 发布 |
| [风险与待决事项](delivery/risks-and-decisions.md) | 13 个未决问题、责任角色和关闭条件 | 产品/技术 |
| [运维恢复手册](operations/runbook.md) | 健康检查、故障处置、灾难恢复和支持诊断 | 支持/核心服务 |
| [ADR 索引](adr/README.md) | 4 个关键方案的替代选择和代价 | 技术 |
| [工作项模板](templates/work-item.md) | 可直接细化开发 issue 的字段 | 开发 |
| [变更申请模板](templates/change-request.md) | 需求/契约/设计变更评审 | 产品/技术 |
| [测试与发布记录模板](templates/test-and-release-record.md) | 测试证据和发行放行记录 | QA/发布 |

## 按角色阅读

- 产品/项目管理：范围基线 → 待决事项 → 开发计划 → 追踪矩阵。
- 核心工程：领域术语 → 架构 → 一致性 → 备份恢复 → 数据与契约 → ADR。
- 前端/设计：领域术语 → 交互规格 → IPC → 状态机 → 测试策略。
- QA/安全：PRD 与追踪矩阵 → 威胁模型 → 测试策略 → 性能兼容 → 发布门禁。
- 发布/支持：发布策略 → 运维手册 → 数据迁移 → 风险登记。

## 建议评审顺序

1. 冻结正式首发范围，解决结构化摘要、环境档案和平台验收要求。
2. 用真实授权样本验证世界边界、外部进程和平台文件系统能力。
3. 评审 4 个 ADR、恢复状态机、格式与 IPC 契约，锁定安全不变量。
4. 由执行团队分配工作包 Owner 和估时，建立 fixture 与第一条端到端切片。
5. 每阶段回填追踪矩阵实现/测试状态，正式发行依据签名产物与测试证据放行。

## 关键不变量

世界级完整范围由边界计划决定；摘要无权删减文件。Git 故障不阻断独立完整备份。覆盖当前状态前必须保护，保护失败中止。校验、采集一致性和游戏兼容性是不同状态。移除索引不删除原文件。未验证的版本、资源和平台始终显示未验证。

本套文档采用企业开发常用的责任分工、追踪、变更和门禁机制，不代表已完成认证、代码安全审计或游戏兼容性验证。

- [游玩实例登记基础与并发前置边界](design/play-instance-registration.md)
- [实例参数试运行与会话关联](design/instance-launch-trial.md)
- [实例试运行验证](quality/instance-launch-trial-verification.md)
- [CDDA 0.H 目录与缓存源码证据](research/cdda-0h-directory-evidence.md)

- [真实游戏绑定与多版本并发验收步骤](quality/real-game-binding-acceptance.md)
- [官方 macOS H/I-1 真实实验记录](quality/real-game-macos-2026-10-28.md)
- [真实样本的固定源码与构建证据](research/cdda-real-game-acceptance-contract.md)
- [目录观测工具](../scripts/acceptance/README.md)
