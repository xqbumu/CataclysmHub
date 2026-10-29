# 简化启动后的真实世界保存与读取验收

日期：2026-10-29。产品基线：`a253495`，macOS Apple Silicon Debug 应用。

## 结论

单个 CDDA 0.I-1 QA 样本完成“Hub 一键启动 → 创建世界与角色 → 移动 → 保存退出 → Hub 一键启动 → 读取同一角色 → 保存退出”的闭环。两次游戏进程正常退出，退出码均为 0。未修改产品代码，也未重新执行单元测试。

## 环境与操作

复用此前 .app 导入验收的隔离工作区、托管实例和已保存普通方案 `cdda-0i-1-macos-native-trial-v1`。安装名为“CDDA I · 实际验收”，实例名为“启动流程验收”；初始 save 目录为空。完整路径、二进制摘要、会话和目录比较见 [result.json](evidence/world-cycle-2026-10-29/result.json)。

QA 包是独立复制的样本；重复的 Resources 程序入口已在本轮基线前重命名，以保留唯一 MacOS 入口。不是未经调整的上游应用包认证。原来源包不变。

1. 在 Hub 点击实例“启动”，无需重新设置启动参数。
2. 游戏中选择 Play Now! (Default Scenario)，生成世界 Betteravia，默认模组为 dda、no_npc_food、personal_portal_storms。
3. 进入 evac shelter B-05，向东移动一格，游戏时间从 08:00:00 推进到 Spring day 61 08:00:01。
4. 从游戏菜单 Save and quit，确认后回到主菜单，再正常退出游戏进程。
5. 从 Hub 再次点击“启动”。Load 列表显示 Betteravia (1)，角色列表显示 Bernie 'Waffle' Hutton。
6. 读取后核对地图位置、08:00:01、walking cane 以及力量8、敏捷11、智力6、感知7。角色信息页再次确认姓名。
7. 再次保存退出，正常关闭游戏和 Hub；PID 65389、68068、70185 均已不存在。

操作中曾将附近 NPC Krystyna Gannon 误认为玩家，随后通过角色加载列表、存档文件名与角色信息页纠正；最终证据均以 Bernie 'Waffle' Hutton 为准。

CUA 曾无法连接后台 Hub 窗口，通过 Finder 打开同一已运行应用后恢复，Hub PID 未变。这里只记录验收工具的激活步骤，不将其解释为产品白屏原因。

## 存档与写入范围

存档位于实例的 `save/Betteravia/#QmVybmllICdXYWZmbGUnIEh1dHRvbg==.sav.zzip`。实际格式为 `.sav.zzip`，没有将其当作 JSON 解码或改写。第一次保存为 20480 字节，SHA-256 `70a4d5bf7b23e2dd0fb944b7d31a4c7a98f0898adde24e7089b4a5839d464929`；读档后再次保存为 17736 字节，最终摘要见 result.json。重保存会改变容器，未要求字节一致。

基线到最终快照：实例范围 124 个路径变化；测试安装、来源安装、哨兵范围各 0 个变化；四个根目录身份均未变化。快照记录文件内容摘要、权限和链接目标，不跟随链接。这是声明目录的前后比较，不是全设备写入追踪或原子快照。

两次会话 PID 分别为 68068、70185，均关联同一实例及方案，最终状态仍为 `exited_unverified`。人工通过本案例不自动提升全局绑定或并发信任标志。

## 证据与边界

截图依次为 [进入世界](evidence/world-cycle-2026-10-29/01-world-entered.png)、[保存后的世界列表](evidence/world-cycle-2026-10-29/02-saved-world-menu.png)、[角色加载列表](evidence/world-cycle-2026-10-29/03-load-character.png)、[读取后的地图](evidence/world-cycle-2026-10-29/04-world-reloaded.png)、[角色信息](evidence/world-cycle-2026-10-29/05-reloaded-character.png)、[最终 Hub 会话](evidence/world-cycle-2026-10-29/06-final-hub.png)。[证据索引](evidence/world-cycle-2026-10-29/index.json)记录文件摘要，result.json 记录本机原始快照位置及摘要；隔离测试世界保留。

本次仅覆盖 macOS 的单个 I QA 样本与默认模组世界。不覆盖多版本并发、H/BN 等其他版本、声音播放、第三方模组、跨版本迁移、备份恢复及 Windows/Linux。后续应以第二个隔离版本执行各自建档与读档，再单独开展受控并发验收。
