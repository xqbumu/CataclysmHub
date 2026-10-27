# 需求追踪矩阵

版本：0.1.0 · 日期：2026-10-27 · 状态：设计覆盖已建立，部分开发切片已实现，详见下方执行记录。责任角色：产品、QA。

## 使用规则

保留原始需求编号与强度；MOD 和 AC 原表没有 MUST/SHOULD 栏，标记“未标注”，不擅自补写优先级。每行的测试是主要覆盖入口，具体逐项验收仍须按原文核对，不能因一个组合用例通过便批量关闭需求。工作包详见 [开发计划](../delivery/development-plan.md)，测试定义详见 [测试策略](../quality/test-strategy.md)。

下表保留设计基线的覆盖入口，不能把一个切片通过测试等同于整条需求已验收。范围后置不删除需求；WP-18 的可选功能启用时才适用相应条件，正式发布范围须参照 [范围基线](scope-and-governance.md) 评审。当前实现与测试状态以 [开发切片记录](../delivery/implementation-status.md) 为准。

## 当前执行记录

| 需求组 | 已落地部分 | 实际验证入口 | 未完成边界 |
|---|---|---|---|
| APP-001/002/004/006 | 工作区配置、SQLite 审计、直接进程与扫描中断状态 | foundation.rs、world_scan.rs 集成测试及原生工作区流程 | 尚非通用任务/恢复 journal 完整实现 |
| APP-003、WORLD-001/002/005/006/007 | 可取消只读枚举、未知关联、共享数据、链接/限制告警、目录候选隔离 | world_scan.rs 2 项单元与 9 项本机集成、WorldsPage.test.tsx 7 项 UI 测试 | 没有已验证游戏适配器、角色解析或权威世界边界 |
| GAME-001/005/006/009 | 本地导入、启动预览、程序摘要和直接进程结果 | foundation.rs、App.test.tsx、原生合成程序启动流程 | 版本识别、子进程接管、发行资产来源验证仍待实现 |
| X-05 | schema 1 → 2 的一致性元数据备份及事务迁移、失败保留 | world_scan.rs 迁移测试及原生旧工作区升级 | 非完整数据库灾难恢复与自动降级工具 |
| BACKUP-001/002/003/004/005/006/007/008、APP-003/004/006 | 存档根目录完整归档、清单/哈希、冻结复制、独立校验、取消与重启记录、源目标分离 | backup.rs 12 项集成、BackupsPage.test.tsx、原生创建/重新校验、独立 Python 读取及 Schema 核验 | 没有已验证世界级选择、自动保留、恢复与完整故障矩阵；正式 AC-006/007 仍待验收 |

以上为阶段性证据，AC-001～013 均未宣布正式验收通过。源码路径以仓库的 `crates/hub-core/tests/` 和 `apps/desktop/src/` 为准；详细限制见 [候选扫描实现](../design/read-only-world-inventory.md)。

## PRD 编号需求与验收

| ID | 原始强度 | 原文要求/验收 | 设计入口 | 工作包 | 主要测试 |
|---|---|---|---|---|---|
| APP-001 | MUST | 首次启动提供工作区初始化：选择游戏安装、存档根目录、备份目录与默认资源目录。 | [设计](../design/ux-specification.md) | WP-02 | TC-APP-01 |
| APP-002 | MUST | 显示游戏、世界、备份和任务状态；状态必须可解释且不依赖颜色单独表达。 | [设计](../design/ux-specification.md) | WP-02 | TC-APP-01 |
| APP-003 | MUST | 耗时操作显示进度、取消策略和结果；取消不能留下未标记的半成品备份。 | [设计](../design/consistency-and-tasks.md) | WP-03 | TC-BACKUP-02 |
| APP-004 | MUST | 所有高风险操作写入本地操作日志。 | [设计](../operations/runbook.md) | WP-03 | TC-AUDIT-01 |
| APP-005 | SHOULD | 支持浅色/深色主题、键盘操作、可缩放布局和高 DPI。 | [设计](../design/ux-specification.md) | WP-02 | TC-UX-01 |
| APP-006 | MUST | 应用崩溃或被强制关闭后，重启时检测并恢复未完成任务状态。 | [设计](../design/consistency-and-tasks.md) | WP-03 | TC-DB-01 |
| GAME-001 | MUST | 支持导入用户已有的游戏安装，不强制重新下载。 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-01 |
| GAME-002 | MUST | 识别操作系统、CPU 架构、游戏版本/分支；无法确认时标记 unknown。 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-01 |
| GAME-003 | SHOULD | 提供发行发现与下载能力，下载前展示来源、文件大小、版本和校验信息。 | [设计](../design/game-content-and-migration.md) | WP-18 | TC-REL-02 |
| GAME-004 | MUST | 允许多个游戏版本并存，不覆盖仍被世界或环境档案引用的安装。 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-01 |
| GAME-005 | MUST | 启动前展示游戏版本、环境档案、存档根目录和启动参数。 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-01 |
| GAME-006 | MUST | 监测由管理器启动的游戏进程；进程退出后进入存档稳定性检查流程。 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-02 |
| GAME-007 | MUST | 更新或替换游戏前，提醒受影响的世界并提供检查点/完整备份选项。 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-01 |
| GAME-008 | SHOULD | 支持启动参数模板和工作目录配置，参数不得静默覆盖用户设置。 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-01 |
| GAME-009 | MUST | 对每个发行资产记录来源 URL、获取时间、校验摘要及安装状态。 | [设计](../design/game-content-and-migration.md) | WP-18 | TC-REL-02 |
| WORLD-001 | MUST | 扫描存档根目录并识别世界候选项、角色候选项及相关文件。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| WORLD-002 | MUST | 将世界与角色关系建模为可变且带置信度的关联；不可靠时显示“未确认”。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| WORLD-003 | MUST | 支持用户为世界设置别名、备注、标签、重要性和测试/主世界分类。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| WORLD-004 | MUST | 默认以整个世界及其全部相关数据作为 Git 检查点和完整备份单位。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| WORLD-005 | MUST | 不同世界不得在默认操作中混入同一个世界检查点。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| WORLD-006 | MUST | 同名角色必须通过世界上下文及可用标识区分，不能只按显示名称合并。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| WORLD-007 | MUST | 检测到目录结构或存档格式无法识别时，保留原始数据并提示人工确认。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| WORLD-008 | SHOULD | 支持主世界、实验世界、迁移测试世界等分类和筛选。 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| GIT-001 | MUST | 每个世界建立独立 Git 仓库，或采用等价的强隔离逻辑；首发优先独立仓库。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-01 |
| GIT-002 | MUST | 初始化仓库前显示目标路径、将纳入的文件和预计空间；不得静默初始化错误目录。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-01 |
| GIT-003 | MUST | 创建检查点前检查游戏进程、文件稳定性和并发写入情况。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-01 |
| GIT-004 | MUST | 检查点包含完整的已纳入世界数据；摘要失败不能减少提交文件。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-01 |
| GIT-005 | MUST | 支持查看提交时间、父提交、文件增删改统计、摘要和原始 diff。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-01 |
| GIT-006 | MUST | 恢复历史前创建保护检查点或完整备份，并展示回滚范围。 | [设计](../design/backup-and-restore.md) | WP-10 | TC-RES-01 |
| GIT-007 | SHOULD | 支持命名标签，例如“升级前”“重要基地完成”“长期存档”。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-01 |
| GIT-008 | MUST | 提供仓库健康检查、对象完整性检查、磁盘空间预警和安全的维护入口。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-02 |
| GIT-009 | MUST | 不得自动执行破坏性 Git 清理或历史重写；必须有显式确认和保护流程。 | [设计](../design/checkpoint-and-summary.md) | WP-07 | TC-GIT-02 |
| COMMIT-001 | MUST | 自动检查点生成可读 commit subject；默认使用中文，允许配置语言。 | [设计](../design/checkpoint-and-summary.md) | WP-08 | TC-GIT-01 |
| COMMIT-002 | MUST | 首版至少生成文件级摘要：新增/修改/删除文件数、主要路径类别和差异规模。 | [设计](../design/checkpoint-and-summary.md) | WP-08 | TC-GIT-01 |
| COMMIT-003 | MUST | 对经过版本验证的存档格式解析角色/世界结构化字段，并生成前后状态差异。 | [设计](../design/checkpoint-and-summary.md) | WP-15 | TC-SUM-01 |
| COMMIT-004 | MUST | 摘要区分事实、推断与未知；不能把推测的探索/战斗/制作行为写成确定事件。 | [设计](../design/checkpoint-and-summary.md) | WP-08 | TC-SUM-01 |
| COMMIT-005 | MUST | 结构化解析失败时退回文件级摘要，且不阻断 Git 提交或完整备份。 | [设计](../design/checkpoint-and-summary.md) | WP-08 | TC-SUM-01 |
| COMMIT-006 | MUST | 每条结构化摘要可追溯到解析器版本、游戏版本和变更类别。 | [设计](../design/checkpoint-and-summary.md) | WP-15 | TC-SUM-01 |
| COMMIT-007 | SHOULD | 用户可在提交前编辑标题和正文，或关闭自然语言摘要。 | [设计](../design/checkpoint-and-summary.md) | WP-08 | TC-SUM-01 |
| COMMIT-008 | SHOULD | 可选 AI 润色只接收最小化结构化报告；默认不上传原始存档。 | [设计](../design/checkpoint-and-summary.md) | WP-18 | TC-SUM-02 |
| COMMIT-009 | MUST | AI 不可用、超时或返回无效文本时，使用确定性模板完成提交。 | [设计](../design/checkpoint-and-summary.md) | WP-18 | TC-SUM-02 |
| COMMIT-010 | MUST | 摘要生成器不得决定纳入哪些文件；文件纳入范围由备份/检查点策略决定。 | [设计](../design/checkpoint-and-summary.md) | WP-08 | TC-SUM-01 |
| WATCH-001 | MUST | 监控存档相关目录变更，并对事件进行去抖和批处理。 | [设计](../design/consistency-and-tasks.md) | WP-06 | TC-WATCH-01 |
| WATCH-002 | MUST | 文件监控事件只触发候选检查点，不直接证明存档已一致。 | [设计](../design/consistency-and-tasks.md) | WP-06 | TC-WATCH-01 |
| WATCH-003 | MUST | 游戏运行时默认不创建标记为“稳定”的检查点。 | [设计](../design/consistency-and-tasks.md) | WP-06 | TC-WATCH-01 |
| WATCH-004 | MUST | 进程退出后，等待配置的静默窗口并检查文件大小/修改时间是否稳定。 | [设计](../design/consistency-and-tasks.md) | WP-06 | TC-WATCH-01 |
| WATCH-005 | MUST | 若检测到文件持续变化、锁定或进程仍运行，延迟快照并解释原因。 | [设计](../design/consistency-and-tasks.md) | WP-06 | TC-WATCH-01 |
| WATCH-006 | SHOULD | 支持用户手动请求“尽力快照”，并醒目标注一致性风险。 | [设计](../design/consistency-and-tasks.md) | WP-06 | TC-WATCH-01 |
| WATCH-007 | MUST | 监控失效、权限不足或目录不可访问时，展示告警且不得假装正在监控。 | [设计](../design/consistency-and-tasks.md) | WP-06 | TC-WATCH-01 |
| BACKUP-001 | MUST | 独立于 Git 仓库生成可单独恢复的完整归档。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-01 |
| BACKUP-002 | MUST | 默认以世界为单位；同时支持整个存档根目录级备份。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-01 |
| BACKUP-003 | MUST | 归档包含文件清单、相对路径、文件大小、SHA-256 和格式版本。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-01 |
| BACKUP-004 | MUST | 建议首发使用 TAR + Zstandard；压缩格式必须版本化且可检测。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-01 |
| BACKUP-005 | MUST | 归档写入临时路径，完成后验证，再发布为正式备份。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-01 |
| BACKUP-006 | MUST | 支持验证归档完整性，不必先覆盖用户现有存档。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-01 |
| BACKUP-007 | MUST | 备份失败必须显示具体原因，例如权限不足、磁盘不足、源文件变化。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-02 |
| BACKUP-008 | MUST | 备份目录默认与源存档目录分离；用户可选择外置磁盘路径。 | [设计](../design/backup-and-restore.md) | WP-09 | TC-BACKUP-01 |
| BACKUP-009 | SHOULD | 支持保留策略：自动检查点、每日、每周和命名检查点分层保留。 | [设计](../design/backup-and-restore.md) | WP-11 | TC-BACKUP-03 |
| BACKUP-010 | MUST | 命名/锁定检查点不因自动清理被删除；最近一次验证成功的备份必须保留。 | [设计](../design/backup-and-restore.md) | WP-11 | TC-BACKUP-03 |
| BACKUP-011 | MUST | 删除备份前展示数量、容量、时间范围和是否为最后一个有效备份。 | [设计](../design/backup-and-restore.md) | WP-11 | TC-BACKUP-03 |
| MOD-001 | 未标注 | 发现本地 mods，并在不破坏原文件的前提下建立索引。 | [设计](../design/game-content-and-migration.md) | WP-12 | TC-CONTENT-01 |
| MOD-002 | 未标注 | 支持资源包安装、启用/禁用和按环境档案保存组合。 | [设计](../design/game-content-and-migration.md) | WP-12 | TC-CONTENT-01 |
| MOD-003 | 未标注 | 在元数据可用时显示依赖、冲突、版本要求；无法推断时明确标记未知。 | [设计](../design/game-content-and-migration.md) | WP-12 | TC-CONTENT-01 |
| MOD-004 | 未标注 | 支持为测试世界建立独立 mod 组合，避免误改主世界配置。 | [设计](../design/game-content-and-migration.md) | WP-12 | TC-CONTENT-01 |
| MOD-005 | 未标注 | 对缺失依赖、重复资源 ID 和已知冲突给出可操作提示。 | [设计](../design/game-content-and-migration.md) | WP-12 | TC-CONTENT-01 |
| MIG-001 | MUST | 支持存档、配置和环境档案的迁移预览。 | [设计](../design/game-content-and-migration.md) | WP-14 | TC-MIG-01 |
| MIG-002 | MUST | 默认不删除源文件；目标验证通过前源数据保持不变。 | [设计](../design/game-content-and-migration.md) | WP-14 | TC-MIG-01 |
| MIG-003 | MUST | 跨平台迁移时检查大小写敏感性、路径长度、权限、符号链接和非法文件名。 | [设计](../design/game-content-and-migration.md) | WP-14 | TC-MIG-01 |
| MIG-004 | MUST | 展示源/目标游戏版本与资源差异，不能静默宣称兼容。 | [设计](../design/game-content-and-migration.md) | WP-14 | TC-MIG-01 |
| MIG-005 | MUST | 迁移失败时保留源数据和诊断日志，清理目标临时目录前先确认其归属。 | [设计](../design/game-content-and-migration.md) | WP-14 | TC-MIG-01 |
| MIG-006 | SHOULD | 支持将世界导出到便携归档并附带环境档案。 | [设计](../design/game-content-and-migration.md) | WP-14 | TC-MIG-01 |
| AC-001 | 未标注 | 可导入首要验证环境中的游戏安装，并显示识别结果与未知项。 | [设计](../quality/test-strategy.md) | WP-04 | TC-GAME-01 |
| AC-002 | 未标注 | 可识别至少一个测试世界；角色绑定不确定时不会伪造确定关系。 | [设计](../quality/test-strategy.md) | WP-05 | TC-WORLD-01 |
| AC-003 | 未标注 | 可创建世界级 Git 检查点，并生成文件级摘要。 | [设计](../quality/test-strategy.md) | WP-07 | TC-GIT-01 |
| AC-004 | 未标注 | 对受支持样本可生成角色/世界结构化差异，摘要能追溯到解析器与字段。 | [设计](../quality/test-strategy.md) | WP-15 | TC-SUM-01 |
| AC-005 | 未标注 | 游戏运行期间不会把自动检查点标记为权威稳定快照。 | [设计](../quality/test-strategy.md) | WP-06 | TC-WATCH-01 |
| AC-006 | 未标注 | 可创建独立完整备份，生成清单并完成 SHA-256 校验。 | [设计](../quality/test-strategy.md) | WP-09 | TC-BACKUP-01 |
| AC-007 | 未标注 | 可在空目录恢复备份，并验证恢复文件与清单一致。 | [设计](../quality/test-strategy.md) | WP-10 | TC-RES-01 |
| AC-008 | 未标注 | 恢复前会保护当前数据；保护失败时默认中止覆盖。 | [设计](../quality/test-strategy.md) | WP-10 | TC-RES-03 |
| AC-009 | 未标注 | 损坏备份、路径穿越和中途退出测试不会导致源存档被覆盖。 | [设计](../quality/test-strategy.md) | WP-10 | TC-SEC-01 |
| AC-010 | 未标注 | Git 仓库不可用或摘要分析失败时，独立完整备份仍可工作。 | [设计](../quality/test-strategy.md) | WP-09 | TC-GIT-02 |
| AC-011 | 未标注 | 离线环境可使用本地游戏库、Git、备份和恢复功能。 | [设计](../quality/test-strategy.md) | WP-16 | TC-OFFLINE-01 |
| AC-012 | 未标注 | 用户可导出环境档案，并在导入时看到缺失资源与版本差异。 | [设计](../quality/test-strategy.md) | WP-13 | TC-PROFILE-01 |
| AC-013 | 未标注 | 所有高风险操作均有审计记录和可读失败原因。 | [设计](../quality/test-strategy.md) | WP-03 | TC-AUDIT-01 |

## 未编号章节要求的派生索引

X-* 为本套文档补充的追踪编号，不冒充 PRD 原有编号。强度以所引原文章节为准。

| 派生 ID | PRD 来源 | 要求 | 设计入口 | 工作包 | 主要测试 |
|---|---|---|---|---|---|
| X-01 | §8.1/8.2 | 共享未知数据显式列出，默认禁止跨世界单角色恢复 | [设计](../design/game-content-and-migration.md) | WP-05 | TC-WORLD-01 |
| X-02 | §11 | 安装与存档分离，引用中的安装不可自动删除 | [设计](../design/game-content-and-migration.md) | WP-04 | TC-GAME-01 |
| X-03 | §12.3 | 音效、图块不同角色与字体来源/绑定/引用保护 | [设计](../design/game-content-and-migration.md) | WP-12 | TC-CONTENT-01 |
| X-04 | §13.1 | 版本化 JSON 档案、缺失资源、未知字段与平台差异 | [设计](../contracts/file-formats.md) | WP-13 | TC-PROFILE-01 |
| X-05 | §14/14.1 | 元数据迁移备份、删除引用不删除文件、DB 损坏独立恢复 | [设计](../design/data-model.md) | WP-03 | TC-DB-01 |
| X-06 | §15.1 | 高风险预览、明确确认、取消无部分覆盖 | [设计](../design/ux-specification.md) | WP-10 | TC-RES-03 |
| X-07 | §17 | 路径穿越、恶意解包、命令注入和解析资源限制 | [设计](../security/threat-model.md) | WP-16 | TC-SEC-01 |
| X-08 | §17.1 | 最小权限、秘密安全存储、默认不上传 | [设计](../security/threat-model.md) | WP-16 | TC-SEC-05 |
| X-09 | §18.1 | 启动/查询/事件响应性能及后台取消 | [设计](../quality/performance-and-compatibility.md) | WP-16 | TC-BACKUP-02 |
| X-10 | §19 | 日志脱敏、任务追踪、导出预览和备份健康 | [设计](../operations/runbook.md) | WP-03 | TC-AUDIT-01 |
| X-11 | §21 | 三平台矩阵、发布校验、签名、升级与格式兼容 | [设计](../delivery/release-and-cicd.md) | WP-17 | TC-REL-01 |
| X-12 | §1.1/20 | 可靠性分子分母、恢复演练与发布阻断 | [设计](../quality/test-strategy.md) | WP-16 | TC-RES-02 |
| X-13 | §3/9.4 | 可选统计、AI 默认关闭、授权与最小输入 | [设计](../design/checkpoint-and-summary.md) | WP-18 | TC-SUM-02 |
| X-14 | §5/23.1 | 独立实现、许可证/品牌/资源分发核验 | [设计](../delivery/risks-and-decisions.md) | WP-17 | TC-REL-01 |
| X-15 | §10.3/10.5 | 清单版本、独立验证、无 Git/DB 仍可恢复 | [设计](../contracts/file-formats.md) | WP-09 | TC-BACKUP-01 |
| X-16 | §6/18 | 离线核心能力与兼容性证据透明 | [设计](../quality/performance-and-compatibility.md) | WP-16 | TC-OFFLINE-01 |

## 覆盖统计与解释

原文提取 84 个唯一编号（其中 AC 验收 13 项），逐项建立设计、工作包、主要测试入口；另补充 16 条未编号章节索引。此统计只证明追踪完整，不表示需求已实现或验收通过。


## 2026-10-27 新目录恢复实现证据

- AC-007 / X-15：独立 CLI 与桌面新目录恢复已实现，精确路径/类型/大小/SHA-256 回读核对；合成归档原生演练通过。实现与边界见 [新目录恢复](../design/new-directory-restore-implementation.md)，自动用例位于 `crates/hub-core/tests/backup.rs` 与 `apps/desktop/src/RestoreWorkflow.test.tsx`。
- X-06 / AC-009：桌面使用过期摘要计划与明确确认；取消和成功标记发布互斥。测试覆盖目标抢占、归档变化、父目录替换、终态持久化失败、重启不重放。仍需真实进程强杀、介质中断与三平台矩阵验收。
- AC-008：当前只创建新目录，不覆盖当前存档；保护备份、受控替换与回滚没有实现，因此不能标记覆盖恢复验收完成。


## 2026-10-27 恢复中断验证补充

- AC-009 / X-06：六个真实核心子进程强杀边界通过，覆盖部分载荷、回读后、报告/完成标记与 DB 提交间隙；重启不重放、不改目标、不误报成功。详见 [强杀矩阵](../quality/restore-crash-matrix.md)。这不代替物理介质和覆盖恢复验收。
- AC-007 / X-15：新增不依赖原元数据的只读 verify-restored，历史报告无法掩盖等长篡改、缺失或额外载荷。字节通过与原流程完成状态分别报告，检查不会自动变更任务结论。
