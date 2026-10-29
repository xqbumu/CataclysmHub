# 模组、声音包与实例管理调研

日期：2026-10-29。目的：指导本轮交互及约束修复；研究建议不等于已实现能力。

## 结论

以“游戏安装版本 → 游玩实例 → 世界”为上下文管理资源。共享资源库只代表来源；资源被发现、配置引用、游戏实际加载是三种状态。实例卡片应直接提供启动与配置入口，启动参数只在当前实例范围内选择，配置页明确展示目标安装及实例。不能根据模组版本字段自动判定兼容，也不能把 Catapult 的共享用户目录模型照搬到并发实例。

## 一手依据

### CDDA 的目录与资源作用范围

本轮读取 CDDA 上游提交 `200b71ee240cb3e694e5561417fa459d200cf854`，以下描述针对该源码快照；不能自动外推为所有 H/I 发行包的适配证明。

- `path_info.cpp` 把存档、配置和用户资源放在用户数据根下，并分别提供 `mods/`、`sound/`；安装资源路径另行提供。默认用户根按 Windows、macOS、XDG 编译选项区分，且调用者可以传入指定根。因此 Hub 应为每个实例提供明确数据根，不能只分离世界目录就宣称配置隔离。[目录实现](https://github.com/CleverRaven/Cataclysm-DDA/blob/200b71ee240cb3e694e5561417fa459d200cf854/src/path_info.cpp#L74-L99)
- 官方 Modding 文档区分安装内置 `data/mods` 与第三方 `mods` / `USER_DIR/mods`；`version` 仅供展示，没有通用版本匹配系统；依赖与冲突是模组 ID 列表。把字符串版本当作语义版本范围会产生错误兼容结论。[Modding 文档](https://github.com/CleverRaven/Cataclysm-DDA/blob/200b71ee240cb3e694e5561417fa459d200cf854/doc/MODDING.md)
- `set_default_mods` 写用户默认组合；每个世界另有 `mods.json`，保存并加载其有序模组 ID。加载中还可能处理缺失、迁移或移除模组。因此“修改新世界默认组合”不能命名为“切换所有世界模组”；旧世界改模组应单独保护与验收。[模组管理源码](https://github.com/CleverRaven/Cataclysm-DDA/blob/200b71ee240cb3e694e5561417fa459d200cf854/src/mod_manager.cpp#L286-L301)、[世界清单](https://github.com/CleverRaven/Cataclysm-DDA/blob/200b71ee240cb3e694e5561417fa459d200cf854/src/mod_manager.cpp#L382-L443)
- 声音包元数据用唯一 NAME 与展示 VIEW；资源文件相对包目录解析。[声音包文档](https://github.com/CleverRaven/Cataclysm-DDA/blob/master/doc/SOUNDPACKS.md)
- 声音发现搜索用户 `sound` 和安装 `data/sound`。`SOUND_ENABLED` 与 `SOUNDPACKS` 属于游戏设置，选包要求重启；候选资源存在不等于已选用，更不等于设备实际播放。[声音发现与选项源码](https://github.com/CleverRaven/Cataclysm-DDA/blob/200b71ee240cb3e694e5561417fa459d200cf854/src/options.cpp#L1237-L1247)、[声音配置](https://github.com/CleverRaven/Cataclysm-DDA/blob/200b71ee240cb3e694e5561417fa459d200cf854/src/options.cpp#L1683-L1701)

### Catapult：借鉴清晰的上下文和状态，保留本项目隔离要求

本轮读取 Catapult 提交 `f90f1cf89ee285f1624baab47e5cc916b727a364`。

- 项目支持多版本选择、模组与声音包安装、存档备份，明确采用便携式布局。[项目 README](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/README.md)
- 路径集中管理；内置 mods/sound 跟随当前安装，用户资源跟随 `<game>/userdata`，不是每个安装分别独立的实例。可借鉴“当前目标安装决定内置资源”的上下文，不能继承其共享目录用于多版本并发。[路径源码](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/scripts/path_helper.gd#L98-L129)
- 模组 UI 分为已安装与可用，提供内置/过时内容筛选和详情；其管理器还区分内置、外置和过时状态。适合借鉴列表层次与来源标识，但 Hub 不应照搬重写 ID 解决冲突的行为。[模组 UI](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/scripts/ModsUI.gd)、[管理器](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/scripts/ModManager.gd#L96-L143)
- 声音包分已安装与可获取列表，内置包禁用删除；安装仍针对用户声音目录。Hub 可借鉴清晰区分来源与目标，不应在未选实例时提供含混的全局“启用”。[声音 UI](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/scripts/SoundpacksUI.gd#L58-L115)、[声音管理器](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/scripts/SoundpackManager.gd#L53-L87)

### CDDA Game Launcher：历史交互参考，不作为当前兼容保证

该项目 README 已明确停止维护。它把模组、声音包与备份作为独立功能，升级时保留旧版本并复制用户数据；其根据 ID 保留新版内置资源的更新策略，不能证明复制后的模组与新游戏版本兼容。只借鉴保留旧数据和明确操作对象的体验，不复制其升级策略作为本项目的兼容模型。[项目状态与升级说明](https://github.com/remyroy/CDDA-Game-Launcher/blob/fa181cb71a502ef36fa5e8dcc1a97c21171b9e28/README.md)

## 本项目现状与本轮最小改进建议

以下为基于源码与当前产品需求的设计推论，不是上游明确要求。

| 现状 | 建议 |
|---|---|
| 资源库只有共享登记列表，配置入口藏在实例卡片的“读取引用” | 内容页增加安装→实例选择，展示目标名称、版本与数据根；配置操作必须绑定具体实例 ID |
| 资源登记与目标生效容易混淆 | 共享来源库保留“登记、安装独立副本”；目标区单独显示“安装内置、实例用户目录、配置引用”，不统一叫已启用 |
| 每页一个启动方案选择影响所有实例 | 点击某个实例的“启动”后打开该实例专属启动配置；先选适配方案，再生成后端预览并确认。切换实例不能沿用另一个实例的确认或计划 |
| 启动、诊断、配置文字偏实现 | 卡片主按钮“启动”，次按钮“配置”；配置含声音、新世界默认模组与已有世界只读声明；资源冲突诊断放入详情 |
| 部分候选诊断不代表兼容 | 可用性依目标实例实际发现；声明兼容范围若将来增加，须绑定安装 ID/构建摘要和资源摘要。未知版本不推断兼容，声明也不能替代真实加载 |
| 配置报告与下拉选择可能变化 | 选安装后只列其关联实例；切换上下文清空报告/草稿，旧异步响应不可套到新目标；应用仍由后端实例 ID、摘要、保护备份及占用校验约束 |

本轮不需要建立自动依赖下载器、通用 semver 引擎、跨版本自动迁移或新的并发调度层。实例资源复制到游戏可发现目录若尚未实现，应清楚标为未完成，不把独立资源副本称作已安装到游戏。已有世界模组修改、删除共享资源、跨版本存档升级/降级继续使用独立变更流程，不能绕过保护备份与真实加载验收。

## 验证建议

1. A/B 两安装、多个实例：所选安装只能配置自己的实例，切换上下文不能显示旧报告或提交旧计划。
2. 同一包可在来源库存在但目标未发现；UI 不显示已启用。
3. 默认模组修改不改变已有世界清单；声音选择只影响所选实例配置。
4. ID 重复、依赖未知、候选资源变化、实例/安装摘要变化仍保留现有后端拒绝行为。
5. A 的启动方案、确认和计划不能被 B 复用；启动前仍展示实际目标和关键限制。
6. 实际游戏加载、声音播放与存档恢复分别验收，不以 UI 状态代替结果。
