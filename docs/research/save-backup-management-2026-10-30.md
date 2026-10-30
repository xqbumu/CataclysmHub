# 存档与备份管理联网调研

日期：2026-10-30。范围：Cataclysm DDA/BN 的存档边界、开源启动器管理方式、备份工具的查询/校验/保留交互。只将一手资料用于结论；本次未运行第三方启动器，不把源码能力当成本项目验收结果。

## 结论

本轮适合优化“从哪个环境来、保存了什么、何时校验、恢复到了哪里”的可见性与连续操作。继续以完整实例根为备份单位，保持新目录恢复和原存档保护；不引入新的备份引擎、角色移植或自动清理。

## 已核实证据

### 1. 世界与角色不是一个文件，也不是跨分支统一格式

CDDA 0.I-1 的 `WORLD::folder_path` 由 save 根和世界名生成，世界有 `world_saves` 和有序模组组合；角色文件基名由角色名 base64 编码。因此相同世界名可以存在于不同实例，目录/角色名不应充当跨实例主键。[CDDA 0.I-1 worldfactory.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.I-1/src/worldfactory.cpp)

该版本的 `game::save` 依次写玩家、成就、世界主数据、地图和其他状态，地图又包含多类数据。由此不能把 `.sav` 数量当完整可恢复存档数，运行中复制文件也不等于一致性快照。[CDDA 0.I-1 game.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.I-1/src/game.cpp)

BN main 的世界结构同时保留松散 JSON 的 V1 与压缩 SQLite 的 V2 格式，并维护世界模组顺序。当前分支源码不等于所有历史发行版，不能将 DDA 目录解释器无条件用于 BN。[BN world.h](https://github.com/cataclysmbnteam/Cataclysm-BN/blob/main/src/world.h)

BN 当前世界实现将地图置于 SQLite，并使用 WAL、保存事务和多数据库提交。不能忽略旁文件、只热拷贝单个数据库，或仅凭 SQLite 存在就宣称整体存档具有事务一致性。[BN world.cpp](https://github.com/cataclysmbnteam/Cataclysm-BN/blob/main/src/world.cpp)

**对本项目的推论：** 世界只标“目录候选”；实例 ID、安装摘要和登记根是管理关联基础。继续完整实例备份、停止游戏后处理，不新增字符级移植、不承诺 DDA/BN/旧版本之间兼容。实际能读档仍需游戏内验收。

### 2. 启动器可借鉴操作组织，不能直接照搬覆盖恢复

CDDA Game Launcher 源码将手动名称备份、启动前/退出后/更新前自动备份配置与历史列表放在备份页；恢复前可生成 `before_last_restore` 备份，但实际恢复会临时改名当前 save 并向游戏目录解压。它也区分压缩后容量与原始文件容量。[CDDA Game Launcher backups.py](https://github.com/remyroy/CDDA-Game-Launcher/blob/master/cddagl/ui/views/backups.py)

Catapult 官方 README 声明支持多版本安装切换、更新时保留用户数据和存档备份。本次未验证其内部备份实现、存档隔离强度或 macOS 行为；这些声明仅支持“版本、用户数据、备份应在同一操作链中可见”，不能证明其可作为本项目安全后端。[Catapult README](https://github.com/qrrk/Catapult/blob/master/README.md)

**对本项目的推论：** 借鉴命名、筛选、相关操作就近展示；继续恢复到新根，再显式登记独立环境。不要复用覆盖当前 save 的恢复方式，也不需要复制另一套自动触发系统。

### 3. 查询与容量必须有清楚的范围

restic 的快照列表可按路径、主机过滤并分组；其列表 Size 表示创建快照时包含文件的大小，不能直接当成压缩后的物理占用。该工具同时区分快照内容枚举和历史快照筛选。[restic 工作仓库](https://restic.readthedocs.io/en/stable/045_working_with_repos.html)

**对本项目的推论：** 按来源展示历史，恢复记录通过 `backup_id` 关联原副本；“备份来源”与“历史筛选”不要暗中互相改变。容量以“原始文件大小”标识；若没有当前磁盘实际占用，不显示预计释放空间。本地 dashboard 只载入最近 100 个副本时，应标“当前载入”，不能称全量历史/总容量。100 条是本项目现状，不是外部工具规则。

### 4. 校验是一次有时间的操作，不是永久健康标记

restic 默认 `check` 不完整读取仓库数据，完整内容检查需 `--read-data`；可选分批检查也不代表单次覆盖所有数据。由此应区分历史完整性结果和当前介质是否可访问。[restic 检查完整性](https://restic.readthedocs.io/en/stable/045_working_with_repos.html#checking-integrity-and-consistency)

restic 恢复支持单独 target；其文档说明就地恢复中断可留下部分恢复状态，并建议先备份当前数据。[restic 恢复](https://restic.readthedocs.io/en/stable/050_restore.html)

**对本项目的推论：** 直接显示最后校验时间；缺失时间就明确“无时间记录”。继续复用恢复前归档校验和恢复后字节回读；历史“通过”不等于现在仍完好，更不等于游戏语义兼容。提供重新校验入口即可，无须自动后台扫描所有档案。

### 5. 保留规则必须按备份集合隔离，并先说明保留原因

restic 按主机/路径分组后分别应用保留策略，以免无关备份集互相挤占；其 forget 与实际回收数据的 prune 分离，并可 dry-run。[restic 删除快照](https://restic.readthedocs.io/en/stable/060_forget.html)

Borg 提供 dry-run 和保留/删除列表，明确提醒未设归档过滤就会考虑仓库内全部归档，不会自动区分其内容；空间释放另需 compact。[Borg prune](https://borgbackup.readthedocs.io/en/stable/usage/prune.html)

**对本项目的推论：** 保留锁应可直接筛选，帮助用户找到不能被规则清理的副本。延续按实例/来源独立计算和保护原因；只做规则预览，不启用自动删除。当前 recent + UTC 日/周窗口并不等价于 restic/Borg 的完整 GFS 语义，界面和文档不得借名称暗示兼容。

## 建议本轮实施顺序

1. 来源管理：显式跳转到该实例历史；恢复列表按原副本来源筛选，并显示关联的原副本。普通工作区来源与独立实例保持区分。
2. 历史可信度：在副本卡显示最后校验时间及保留锁；允许按锁筛选。未知实例关联保留原 ID，不猜测或合并同名实例。
3. 容量与范围：标识原始文件大小以及当前载入条数，筛选汇总仅覆盖已载入记录；不能暗示全库历史和物理磁盘容量。
4. 验证：前端覆盖来源切换不改变创建目标、恢复记录关联、锁筛选、载入范围与校验时间缺失。保留已验证的备份/恢复和资源占用回归。

## 暂不实施

- 不接入 restic/Borg 后端、去重仓库、云同步、定时调度服务或新数据库索引。这些均需独立成本与迁移论证。
- 不依据 `.sav` 文件名或世界名直接提供角色转移、单角色恢复或跨变体合并。
- 不将校验通过升级为绑定认证，不在游戏运行期间复制，不自动删除副本，不覆盖恢复。
- 本文研究不替代新发行版格式分析，也不扩张已验收平台和自动备份支持范围。

## 本轮实现对应关系

源码核对 `apps/desktop/src/BackupsPage.tsx`：已增加“查看此来源的副本与恢复记录”显式跳转，历史来源约束已结束恢复任务且保留活动任务可见；增加保留锁/已命名筛选、上次成功校验时间以及“原始文件（非归档占用）”标识。查询边界明确为最近 100 份副本和最近 20 条恢复任务，计数按已载入记录计算。这些改动对应上述建议 1～3，没有更换后端、修改保留算法或新增删除能力。

本研究代理只核对网页与改动源码，未执行本轮测试或原生界面验收；具体测试与验收结果以本轮质量记录为准。
