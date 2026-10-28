# macOS 原生入口诊断与 Hub I 游玩验证

日期：2026-10-28，Asia/Shanghai，Apple Silicon。结论：已定位一个可重复的入口位置/原生识别差异，新增显式原生入口方案；固定 I QA 样本已走通 Hub 启动、创建世界、保存、重载、再保存及正常进程退出。**没有据此开放并发或标记完整隔离已验证。**

## 诊断反馈与假设

先按 Resources 本体、父进程环境、工具识别限制三个方向排序，建立同包同字节入口对照。两个入口 SHA-256 均为官方 I 本体的 `3f37ff041654edbb88773666344be0bf8eba67cb44d71c15908aeb961d4aac3a`，Resources 工作目录与环境设置一致，各使用新的空实例根。

- Resources 本体直接运行 PID 52064，原生应用列表无可选游戏，按本体路径定位 Invalid app。结束后从 MacOS 中同字节本体启动 PID 52118，使用完整包路径定位成功，显示真实游戏菜单；确认没有额外启动游戏进程。通过原生快捷键退出，退出 0。
- 为避免使用不同定位引用作比较，在 Hub 游玩实验结束后再次直接运行 Resources 本体 PID 66706，使用完全相同的 `.app` 定位引用，返回 timeoutReached，未观察到第二个游戏进程；只向该自建测试子进程发送 SIGTERM，退出 0。

结果支持在本机该包中使用已有 MacOS 本体获得原生识别；不证明 macOS 内部完整原因，不泛化到所有 bundle/OS。父环境中抽查 __CFBundleIdentifier、SDL_MAC_BACKGROUND_APP、SDL_VIDEODRIVER 均未设置，XPC_SERVICE_NAME=0；没有为了通过实验修改系统环境或激活策略。

Rust 测试没有自动化验证 macOS 窗口识别；这一层以原生工具对照复验，核心回归只验证执行/范围/拒绝不变量，不能冒充 GUI 测试。

## 实现回归

新增 3 个 macOS 核心用例：原生实际本体 + Resources cwd + 实例运行环境 + 直接会话退出；拒绝脚本及预览后程序变化；拒绝 Resources 包外重定向。跨平台拒绝用例覆盖新方案。桌面测试覆盖选方案后重新勾选、传递新方案 ID、预览环境及取消不启动。既有 v1/v2 行为保留。

Rust workspace 177 项通过，前端 55 项通过；fmt、类型/production build、Clippy -D warnings、macOS debug .app 打包通过。首轮与打包并行时 rustdoc 报依赖产物无法定位；打包结束后串行完整重跑通过，未禁用或跳过文档测试。后续此类构建与测试按顺序执行。

## Hub 原生 I 游玩流程

测试包是此前已调整 Info.plist 并增加 MacOS 同字节入口的私有 QA 包，含已生成缓存；不是原始官方发行包。此次产品代码没有重写包。安装与实例记录在独立 schema-10 QA 元数据中人工准备，导入/登记流程没有重新验收；从计划生成开始，以下操作均走最新打包 Hub 原生界面：

1. 明确选择 I 原生本体入口方案并勾选；预览实际 MacOS 程序、Resources cwd、独立实例根与环境，实例仍为空。
2. 确认后 Hub 启动直接子进程 PID 59593；原生工具定位同一游戏，未另起启动器。全局会话保护生效。
3. 游戏中新建默认场景并进入世界 Kaibito，生成角色 Galen Nielsen；等待两回合到 Spring day 61 08:00:02。通过游戏菜单 Save and quit 和大小写敏感确认保存，返回自身加载菜单。
4. 加载菜单明确列出 Kaibito / Galen Nielsen；选择并重新进入原场景，时间仍为 08:00:02。再等待一回合到 08:00:03，再次保存退出至菜单。
5. 从菜单正常关闭游戏，Hub 记录同 PID 为 exited_unverified、exit_code=0，界面无活动资源占用；绑定标记仍 unverified。正常关闭测试 Hub，未留下游戏进程。

角色存档实际文件为 `save/Kaibito/#R2FsZW4gTmllbHNlbg==.sav.zzip`，最终 36864 字节；实验未向文件系统写入伪造世界或角色，也未加载玩家真实存档。此前尝试 Q 快捷键未被识别，随后使用菜单实际保存，未将该按键尝试计为成功。

## 范围观察

观察范围包括安装、Hub I 实例、两个入口对照实例及保护哨兵；证据输出位于这些范围之外。

| 阶段 | 变化范围/条目 | 其他声明范围 |
|---|---|---|
| 新建与首次保存 | instance-i 100 | 未观察到变化 |
| 重载、再次保存和正常退出 | instance-i 26 | 未观察到变化 |

两次 diff 都为 within_declared_allowance；debug.log 中 Stale game data detected 为 0。安装缓存已预热，本轮采集点没有观察到安装变化，不代表游戏永不写安装。快照不是 OS 写入跟踪，不能排除瞬时或范围外写入。

## 证据与剩余范围

[摘要索引](evidence/native-entry-hub-i-2026-10-28.json)保存会话、PID、角色/世界、阶段计数、各原始证据 SHA 和限制。原始资料在 `/private/tmp/hub-window-qa-haihv653`，临时目录不是长期归档；不提交游戏本体或真实存档。

H 与跨进程重载的后续结果见下一节。原始官方脚本包的原生入口准备、故障/子进程接管、实例备份协调、其他系统写入以及 Hub 多版本同时游玩仍未验收；全局互斥不解除。


## 三提交审查后：H 游玩与 H/I 跨进程重载

审查 fa87eab → eecfc56 发现并修复原生入口遗漏 MacOS 父目录物理身份固定的问题，见 [审查结论](review-2026-10-28.md)。仅在既有身份元组增加句柄及一个目录替换回归，无新框架、schema 或参数方案。Rust workspace 178 项、fmt、Clippy -D warnings 和 macOS debug .app 串行通过；本轮前端源码未改。

I 先使用 eecfc56 打包程序重新启动 Hub，再显式启动原实例：新 PID 84453 加载 Kaibito / Galen Nielsen，场景与 Spring day 61 08:00:03 恢复；继续一回合到 08:00:04，游戏菜单保存并正常关闭。Hub 记录 exited_unverified、exit_code=0，资源释放。此阶段未使用随后修复的构建。

随后关闭 Hub、重新构建含身份修复的 .app，再启动 H。H 从已有 QA 样本独立复制，程序 SHA 保持 ba92c296d23758eab4fe4a74ee5ee624f01edaac051c3777c3c138cac45de21d；仍是已有 MacOS 本体、调整 plist 且含预热缓存的本地封装，不是原始官方脚本包。H 安装与空实例在专用元数据中准备，未重新验收导入登记 UI。

1. Hub 预览明确展示 H 本体、Resources cwd、hub-h 数据根与固定环境；确认后直接进程 PID 91950。首次按包路径定位窗口 timeoutReached，应用列表已注册 Hub QA H；按现有 bundle ID 定位成功，未另起游戏。
2. 默认场景进入世界 Hardtner，角色 Lashonda Kohler；推进至 Spring day 61 08:00:01，通过菜单保存，返回加载菜单后正常关闭，会话退出 0、资源释放。
3. 再次由 Hub 预览确认，启动新 PID 93662。加载菜单明确列出 Hardtner / Lashonda Kohler；进入原避难所场景，时间为 08:00:01。继续到 08:00:02，再次保存并正常关闭，会话退出 0。Hub 显示无活动资源占用，最后正常关闭 Hub。

五个观察范围在每个采集点保持一致：I 完整包、H 完整包、I 实例、H 实例和保护哨兵；仅允许当前版本包和实例变化。

| 阶段 | 实际变化范围/条目 | 其他四个声明范围 |
|---|---|---|
| I 跨进程读取、继续保存与退出 | instance-i 31 | 未观察到变化 |
| H 新世界、保存与退出 | instance-h 95 | 未观察到变化 |
| H 新进程读取、继续保存与退出 | instance-h 64 | 未观察到变化 |

三次 diff 均 within_declared_allowance，H/I debug.log 的 Stale game data detected 均为 0。没有加载玩家存档或伪造保存文件。该结果只证明固定 QA 样本的顺序游玩流程；快照不能证明完整隔离，不授权并发或自动提升绑定。

[证据索引](evidence/native-entry-h-i-restart-2026-10-28.json)记录构建差异、会话、世界/角色、时间、最终存档摘要和原始证据 SHA。下一步先明确可复现的原始发行包入口准备及根外写入边界，再决定最小按范围准入变更；未知范围继续全局互斥，避免预先建设泛用租约框架。
