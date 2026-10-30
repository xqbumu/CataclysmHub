# macOS 原始应用包启动与授权排障

日期：2026-10-30。基线：2860ead。平台：macOS Apple Silicon。

## 复现和排除

原始 0.I-1 官方 DMG 的摘要已与官方目录匹配；使用保留时间戳的独立 Game-QA.app，不使用玩家存档。Hub 直接启动 Contents/Resources/cataclysm-tiles，并传递实例 userdir、savedir 和 configdir，使用现有 v2 运行环境。

此前多次进程被 syspolicyd 明确记录为 Gatekeeper rejection。11:00 重试进程 86841 仍被拒绝，系统设置的 Security 区域没有对应的 Open Anyway。仅凭被信号终止不能自动归因到 Gatekeeper；本轮结论依据系统日志。`spctl --assess --type execute --verbose=4` 对该包返回 rejected / no usable signature，这并非已证实恶意软件，也不能据此保证系统一定提供用户例外按钮。

官方 Cataclysm.sh 没有转发 `$@`，因此直接打开整个原包不能替换实例启动。另用 NSWorkspace.openApplication 测试明确的本体 URL（只传 --help），回调返回的是 Terminal.app 的进程和 executableURL，而非游戏本体；`open` 返回成功也不能作为游戏 PID 或参数隔离证据。该候选不进入产品。

这些结果修正了此前“系统设置一定有仍要打开”的不准确引导；无法显示正常授权入口时应保留诊断，不反复要求用户找不存在的按钮。

## 本轮有限授权

用户明确批准后，仅对本轮临时 Game-QA.app 递归移除 com.apple.quarantine。此举绕过该副本的首次隔离检查，**不是已修复正常系统授权弹窗的证据**；没有关闭全局 Gatekeeper、重新签名、修改游戏字节或更改用户原下载文件。操作前后的本体 SHA-256 均为 3f37ff041654edbb88773666344be0bf8eba67cb44d71c15908aeb961d4aac3a。产品代码不会执行该操作，也不将其推广为自动修复。

随后通过 Hub 的原受控启动路径启动会话 fb2d0b33-8848-4c46-8367-143b2b2b2105，PID 95369。实例 config 下生成 options、fonts、debug 等文件；debug.log 记录真实 0.I 版本与 SDL 初始化。用户确认看到主菜单，正在创建测试存档。游戏内操作为用户执行，工具未识别该 Resources 本体为可操作应用，不能冒充自动化验收通过。

## 产品修复

- 保留直接受控本体启动，不改写原包、不调用丢参数的脚本，也不使用 Terminal PID 监测游戏。
- 对无数值退出码的信号终止记录 PROCESS_TERMINATED_BY_SIGNAL。UI 用可读说明展示，不宣称安全策略原因；现有并发候选的未知退出处理保持。
- macOS 首页异常卡片内提供折叠的授权排查说明，明确没有按钮时不要反复重试，也不要通过双击原包绕开实例目录参数。仅失败后显示，不增加首次游玩的选项。
- 不改变会话状态、资源占用、程序摘要、实例目录身份、备份和恢复保护规则。

## 官方依据

Apple 的 [安全打开 App](https://support.apple.com/zh-cn/102445)描述正常的逐应用授权入口，并未保证每种拒绝都有该入口。[NSWorkspace.openApplication](https://developer.apple.com/documentation/appkit/nsworkspace/openapplication(at:configuration:completionhandler:))返回打开的应用；本轮实验明确核对其 executableURL，避免把 Terminal 当成游戏。[Apple DTS 排障讨论](https://developer.apple.com/forums/thread/767612)也说明隔离来源与创建方式可影响启动拒绝，不能只看是否存在隔离属性就归因。

## 检查结果与待续验收

- `npm run check`：152 项前端测试通过，格式、类型和生产构建通过。
- Rust 格式及全特性 Clippy 通过。首次全量测试中备份取消/冲突测试等待超时；同期进行了 Debug 编译，尚不能仅据此断言因果。该用例单独复验通过（3.96 秒），停止打包竞争后的 `cargo test --workspace --all-features --locked` 全量 301 项通过，未放宽门禁或修改测试超时。
- `task build:debug` 通过，普通 Debug 应用 67.15 MiB；未替换用户安装。测试新增后仅重跑检查，产品内容与已打包版本相同。
- 新诊断的自动化回归覆盖信号终止实际子进程、操作日志文案及异常卡片说明；重打包后的新文案尚未进行原生窗口复验。当前原 Hub 会话保持运行以监测用户正在操作的游戏，不为换包中断会话。
- 用户已确认主菜单，但尚未报告保存退出。当前仍为活动会话；未在运行中备份、扫描认证、改配置或恢复。待用户保存退出后核对实际 save 文件与退出记录，再做重载和保护副本流程。
- **完整正常系统授权入口仍未解决或验收通过**。本轮临时副本的成功启动依赖用户明确授权的隔离属性移除，不能宣传为正常 macOS 授权流程已修复，也不自动传播到任何其他安装。
