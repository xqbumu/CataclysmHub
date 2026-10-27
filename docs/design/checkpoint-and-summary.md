# Git 检查点与差异摘要设计

版本：0.1.0 · 日期：2026-10-27 · 状态：待评审 · 责任角色：历史与摘要模块负责人。需求：GIT-001～009、COMMIT-001～010。

## 仓库隔离和完整性

每世界独立仓库存放于管理器工作区，世界 ID 为稳定随机标识，不以目录名生成。Git 只接收冻结副本，源存档从不作为 Git worktree。仓库对象不能依赖用户全局 ignore、hooks、过滤器、自动换行或外部 diff；初始化使用受控配置和空模板。实现阶段验证支持的 Git 版本及相关配置，不假定系统 Git 已存在。

为了完整保存任意 `.git`、`.gitattributes`、保留路径和空目录，建议仓库存储清单与按 SHA-256 命名的原始内容载荷：`snapshot/manifest.json`、`payload/{sha256}`。当前 tree 只引用当前清单需要的载荷；历史对象由历史提交保留。这样提交文件数不同于玩家文件数，UI 必须依据清单计算实际路径增删改，并通过原始字节比较提供 diff。代价是 Git 对改名和内容差分压缩的收益可能下降，M0 需测量容量增长；方案见 ADR-0001。

提交后从 Git tree 读取每个载荷，核对清单集合、字节和哈希后才标记检查点 verified；对象完整性检查用于补充，不能替代载荷与清单比对。未知文件同样纳入。无内容变化返回 no_change 并记录触发，不制造虚假变化提交。

## Git 调用与健康

进程启动使用固定可执行路径与参数数组，选项与路径边界显式分离，拒绝用户提交任意 Git 子命令。清理继承的 Git 环境变量，设置受控 HOME/配置上下文的实现需验证；不要继承不受信任 hooks、pager、diff driver、credential helper 或网络 remote。仓库默认不配置远端。

健康检查包括可用性、版本、磁盘、锁归属、对象检查、最近 verified commit 可读取。锁文件不能因“太旧”直接删除；先验证无所属进程或恢复任务。自动操作不 reset、clean、rebase 或删历史。用户请求维护时先生成计划、保护和影响预览。

Git 恢复先物化为隔离载荷并验证，再调用统一恢复引擎；不直接 checkout 覆盖源世界。Git 损坏时独立备份任务仍运行；同次操作在 UI 分别显示“历史失败”和“完整备份成功”。

## 摘要管线

输入固定为前后冻结清单及只读载荷；输出 `DiffReport`，包含 report_id、world_id、base/current snapshot、文件统计、facts、inferences、unknowns、parser_id/version、game_version、evidence。每条事实必须有前后哈希、相对路径和字段定位；字段不能解析则进 unknowns，不补造值。

解析器按游戏分支、版本及 fixture 证据显式注册，未知格式不使用“最接近”解析器自动认定兼容。大文件、深层对象或损坏数据在独立预算内处理；超时/失败回退文件摘要。对 JSON 等文本不得重新序列化后覆盖源数据。

文件摘要至少展示新增、修改、删除、未知类别数量及字节变化。结构化摘要用“位置字段由 A 变为 B”，不能写“战胜敌人”之类未经记录的行为。推断默认关闭；用户开启后单独标注。

```text
存档更新：{world_alias}（新增 {added}，修改 {modified}，删除 {deleted}）

事实：{facts_or_file_summary}
未知：{unknowns}
World-ID: {world_id}
Consistency: {consistency}
Analyzer: {parser_version_or_file_only}
Report-ID: {report_id}
```

用户修改 commit 文案不覆盖原始 DiffReport；记录 edited 标志。关闭自然语言摘要仍保留基础机器元数据。长文本截断显示，不截断底层证据。

## 可选 AI

M6 后独立启用，发送前展示提供商和最小化报告，明确授权；不外发源文件、完整路径或角色身份。AI 无权修改事实清单、操作范围、校验状态或任务计划。建议超时预算 5 秒，失败回退模板；精确参数在实现时配置化。输出以 evidence ID 对齐，新增事实、控制字符、过长内容或无法核对时拒绝。远程服务不可用不能影响快照完成。
