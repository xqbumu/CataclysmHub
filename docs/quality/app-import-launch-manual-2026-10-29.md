# .app 导入与简化启动：原生桌面实际验收

日期：2026-10-29。基线：`64c9925`，macOS Apple Silicon，`task build:debug` 普通特性实际打包。结果：本测试样本的应用导入及主菜单级启动流程通过，发现两项 P2 文案问题；不代表全部 .app、其他游戏变体或存档加载验收完成。

## 样本与隔离

使用已有 I QA 封装的全新完整副本，程序 SHA-256 为 `3f37ff041654edbb88773666344be0bf8eba67cb44d71c15908aeb961d4aac3a`。该封装的标识和入口经过 QA 调整，不是未经修改的官方发行包认证。

原 QA 包同时有 MacOS/cataclysm-hub-qa 与 Resources/cataclysm-tiles，先用于模糊入口拒绝验证；然后仅在新副本中将后者改名为 cataclysm-tiles.qa-reference，保留唯一原生入口。启动观察基线在这次夹具调整之后建立，原 QA 来源未修改。没有通过改写数据库来导入、配置方案或创建会话。

Hub 使用 CATACLYSM_HUB_DATA_DIR 指向新隔离元数据，系统推荐工作区随之隔离；安装、实例、保护哨兵均在本轮临时目录。首次按应用名称定位工具自动打开了默认已安装 Hub，只读观察后退出；随后以明确环境变量启动本轮构建，确认游戏库为空后执行验收。没有替换 ~/Applications 中的应用。

原始截图、AX 文本、阶段清单与会话记录位于 [证据索引](evidence/app-import-launch-2026-10-29/result.json) 所列临时目录；关键截图与结果已归档进仓库。

## 实际操作结果

| 项目 | 实际观察 | 结果 |
|---|---|---|
| 原生选择器选择整个 .app | 原生 Open 面板将 Game.app 显示为 Application，可直接选中 | 通过 |
| 两个入口时导入 | 显示“未找到唯一游戏入口”；只读查询 installation 数量仍为 0 | 通过 |
| 唯一入口时导入 | 再次导入成功，登记路径为 Contents/MacOS/cataclysm-hub-qa | 通过 |
| 创建托管实例 | 通过界面登记唯一 instances/UUID 数据目录 | 通过 |
| 首次自动推荐 | 显示 CDDA 0.I-1、预选 macOS 普通方案；未确认时 session 数量为 0 | 通过 |
| 首次确认启动 | 展示实例与 save 目标，技术参数默认折叠；确认后实际显示游戏 0.I-1 主菜单 | 通过 |
| 正常退出并保存方案 | 主菜单 q → Y 正常退出，exit_code=0；Hub 偏好保存对应实例方案 | 通过 |
| 重启 Hub 后再次启动 | 同一隔离元数据重新启动 Hub，实例卡点击一次“启动”，无需设置或确认窗口，实际再次显示游戏主菜单 | 通过 |
| 游戏运行中重复启动 | 显示资源冲突阻止信息，没有创建第三条会话 | 通过 |
| 结束清理 | 两条会话均 exited_unverified / exit_code=0；正常退出 Hub，测试游戏与 Hub 进程结束 | 通过 |

两次真实进程 PID 分别为 30195、31721；会话 ID、实例关联、方案及时间见证据索引。exited_unverified 保留原语义，没有把游戏退出提升为存档一致或绑定已验证。

截图：[模糊入口拒绝](evidence/app-import-launch-2026-10-29/01-ambiguous.png)、[导入结果](evidence/app-import-launch-2026-10-29/02-imported.png)、[首次推荐](evidence/app-import-launch-2026-10-29/03-recognized.png)、[重启后直接启动](evidence/app-import-launch-2026-10-29/08-direct-start.png)、[实际游戏主菜单](evidence/app-import-launch-2026-10-29/09-second-game.png)、[重复启动阻止](evidence/app-import-launch-2026-10-29/10-duplicate-blocked.png)。

## 目录观察

在首次启动前、首次退出后、第二次退出后记录声明目录的文件摘要、大小、权限、链接目标及根身份；链接不遍历。测试安装和原 QA 来源各 12,444 项、保护哨兵 1 项，前后没有内容/类型/权限变化，根身份一致。空实例变为 24 项，新增内容位于 config、.hub-runtime 及固定玩家数据目录；包括 options.json、字体、颜色、样式、日志与配置缓存。

这提供本样本的配置输出位置证据，未创建或加载世界，不证明存档绑定、角色恢复或世界语义兼容。观察只覆盖声明目录，不是全机写入归因，也不能排除阶段之间的短暂变化。未修改绑定或并发信任标记。

## 待修问题

- **QA-LAUNCH-001 / P2**：首次设置已识别版本后，确认窗口仍显示“没有识别或验证本地程序版本”。识别结果与启动认证应分别表达，当前文案容易误导用户重复核对。见 [确认窗口 AX](evidence/app-import-launch-2026-10-29/04-first-confirmation.ax.txt)。此外该窗口仍有较长的技术警告，后续宜突出必要风险、折叠解释。
- **QA-LAUNCH-002 / P2**：导入完成提示及安装卡仍显示 version unknown，实例启动设置却已匹配 H/I 样本。安装目录信息与只读识别结果尚未汇合，视觉上不一致。见 [导入 AX](evidence/app-import-launch-2026-10-29/02-imported.ax.txt)。不应为消除 unknown 而把任意名称或侧车当作版本证明。

本轮交付为实际验收记录，未修改产品代码。没有重新执行 Rust/前端单元测试；执行了最新 Debug 实际打包、原生 UI 操作、真实游戏主菜单启动/退出、只读数据库核对和目录阶段比对。原始发行包、H 样本、世界加载、音频、其他系统及多版本并发需另行验收。
