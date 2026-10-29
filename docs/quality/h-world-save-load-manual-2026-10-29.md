# CDDA H 独立实例建档与读档验收

日期：2026-10-29；基线 `2622253`；macOS Apple Silicon，沿用 a253495 构建的 Debug 应用。

## 结果

通过 Hub 原生界面完成第二个版本的 .app 导入、H 样本识别、独立托管实例创建、首次确认启动、建档保存、正常退出、一次点击再次启动、读取原角色并正常保存退出。两次会话 PID 77762、79343，退出码均为 0，状态保持 `exited_unverified`。Hub PID 76356 也已退出。

世界 Monette，角色 Freeman Bergman（Male / Urban Explorer），地点 evac shelter A-23。向东移动一格后 Spring day 61 08:00:01，读取后位置、时间及属性 6/6/10/7 保留。附近 NPC 为 Parker Stiles；没有将 NPC 名称作为玩家身份。角色信息页及加载列表均核对姓名。

默认世界模组为 dda、no_npc_food、personal_portal_storms、no_fungal_growth。存档实际为 `save/Monette/#RnJlZW1hbiBCZXJnbWFu.sav`，最终 83362 字节；摘要和完整位置见 [result.json](evidence/h-world-cycle-2026-10-29/result.json)。本次没有加载 I 世界，也没有迁移跨版本存档。

## 样本和流程

从已有 `/private/tmp/cataclysm-hub-real-game-validation/0.H-RELEASE.app` 复制到同一隔离验收区的 H-Game.app。该样本含此前调整的原生 MacOS 本体入口及 QA bundle ID，重复 Resources 入口在本轮基线前重命名。游戏本体 SHA-256 为 `ba92c296d23758eab4fe4a74ee5ee624f01edaac051c3777c3c138cac45de21d`，窗口自报构建 78fd6e3。不是原始上游发行包认证，样本历史见 [来源记录](real-game-macos-2026-10-28.md)。

Hub 使用既有隔离 CATACLYSM_HUB_DATA_DIR，保留 I 实例；导入名“CDDA H · 实际验收”，新实例“H 存档验收”，普通方案 `cdda-0h-release-macos-native-trial-v1`。界面正确推荐 H 方案，首次预览确认后启动；第二次只点击实例启动。未通过数据库写入绕过界面或提升信任标记。

游戏按可见菜单选择 Play Now! (Default Scenario)，通过 Escape → Save and quit → Y 保存，返回主菜单后 q → Y 正常关闭；第二次在 Load → Monette → Freeman Bergman 读取，角色页核对后按相同流程退出。没有重复设置启动参数。

## 写入观察

| 声明范围 | 基线到最终变化路径数 | 结果 |
| --- | ---: | --- |
| H 测试安装 | 5672 | 全部位于 Resources/data/cache 或 Resources/cache |
| 所有实例 | 120 | 全部属于新 H 实例，I 实例不变 |
| I 测试安装 | 0 | 不变 |
| H 来源包 | 0 | 不变 |
| 测试哨兵 | 0 | 不变 |

所有根目录身份不变。H 本轮会更新安装内 JSON/gfx 缓存，不能宣称游戏安装只读；I 上轮没有变化不代表其他版本也没有变化。完整安装范围应继续参与并发冲突检查。

快照记录权限、文件摘要与链接目标，不跟随链接；只比较声明目录，非原子快照或全设备写入追踪。原始阶段快照位置与摘要在 result.json，观察脚本一并归档；脚本含本机夹具位置，是本次证据，不是通用验收命令。隔离世界保留。

## 证据与后续

[截图与摘要索引](evidence/h-world-cycle-2026-10-29/index.json)：01 为首次角色页，02 为保存前场景，03 为保存后的世界列表，04 为重启角色加载列表，05 为加载后场景，06 为重载角色页，07 为 Hub 最终会话。

结合 [I 验收](world-save-load-manual-2026-10-29.md)，H/I 各自通过普通方案独立建档与读档。本轮未进行同时运行、声音、第三方模组、备份恢复或 Windows/Linux 验收；不自动提升绑定或并发状态。下一步使用已有受控实验构建方案，核对完整安装与实例范围后执行双版本交替操作、分别保存和重载，保持普通方案全局互斥。

本轮仅归档实际验收证据，未修改产品源码；git diff --check 通过，没有重复运行单元测试。
