# macOS 从 .app 启动的复查

日期：2026-10-29。范围：Apple API、当前本机系统手册、固定 CDDA H/I 源码及合成参数实验；本文初版未执行真实游戏或 LaunchServices；随后补充的本地 API 实验见下文，不修改产品代码。

## 结论

**可以直接打开 .app，适合作为“按游戏默认方式打开”的入口；目前不能用它无条件替换 Hub 的实例启动。** 障碍不在 macOS 能否打开应用，而在指定版本的包内脚本是否把目录参数交给游戏，以及 Hub 是否监测到真正的游戏进程。用户不应承担这些后端适配细节。

Apple 提供 `NSWorkspace.openApplication(at:configuration:)`，按明确应用路径启动并返回 `NSRunningApplication`。配置支持新实例参数、环境变量和 `createsNewApplicationInstance`；默认会复用已有应用实例。因此未来从应用包启动可优先用系统 API，不能只凭成功回调认定存档绑定或多实例隔离成立。[启动 API](https://developer.apple.com/documentation/appkit/nsworkspace/openapplication(at:configuration:completionhandler:))、[arguments](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration/arguments)、[environment](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration/environment)、[新实例开关](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration/createsnewapplicationinstance)。

Apple 文档另明确：调用方位于 App Sandbox 时，arguments 和 environment 会被忽略；若以后改变发行方式，需要重新核对。`NSRunningApplication.processIdentifier` 是应用 PID，可能返回 -1，且不能仅凭 PID 比较进程身份；它不会自动成为所有后代进程的可靠身份或退出状态。[参数约束](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration/arguments)、[环境约束](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration/environment)、[进程标识](https://developer.apple.com/documentation/appkit/nsrunningapplication/processidentifier)。

## 当前 H/I 包的具体障碍

固定 H-RELEASE 和 I-1 官方源码的 Info.plist 均指向 `Contents/MacOS/Cataclysm.sh`。两份脚本均切入 Resources 并设置动态库环境，但图形分支以 `./cataclysm-tiles` 启动，**没有转发 `"$@"`，也没有使用 exec**；终端分支则交给 Terminal。因此 `open ... --args --userdir ... --savedir ...` 最多把参数送到包的启动入口，不能替脚本补上参数转发。[H plist](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/build-data/osx/Info.plist#L4-L5)、[I plist](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/build-data/osx/Info.plist#L4-L5)、[H 脚本](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/build-data/osx/Cataclysm.sh#L14-L20)、[I 脚本](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/build-data/osx/Cataclysm.sh#L14-L20)。此结论限定为这两个固定提交，不泛化到其他变体或未来包。

本机 macOS 27.0.1 的 Apple `open(1)` 手册确认：`-n` 请求新应用实例，`-W` 等待打开的应用退出，`--args` 将余下参数传给应用入口，`--env NAME=VALUE` 追加环境。`open` 的退出状态不能直接当作游戏子进程退出码，`-W` 也不会修复脚本丢参数。可在本机用 `man open` 核验；网络一手 API 见上方 Apple 链接。

主实施代理还用本地 I QA 包的 Cataclysm.sh 原字节，在独立临时包内用自建参数接收器替代游戏本体，并传入 userdir/savedir。结果退出 0，接收器 `argc=0`，与源码一致。[归档结果与脚本摘要](../quality/evidence/macos-app-launch-argv-2026-10-29.json)。这是脚本参数转发实验，**未运行真实游戏、未调用 LaunchServices，也不证明官方完整包的实际启动结果**。已有 QA 包的 Info.plist 曾改为 `cataclysm-hub-qa`，不能用其 plist 冒充官方脚本入口。

## 最小简化建议

1. **默认界面只保留“启动游戏”和实例选择。** 对已识别样本自动选择一个内部适配器；将 H/I、v1/v2、native 等技术方案从普通用户流程移走，不再让“游戏版本”下拉框实际承担 profile ID 选择。
2. **不再增加普通用户选项。** 系统直接打开原 .app 与“启动此实例”具有不同语义：前者通常采用游戏自身默认目录，不能承诺 Hub 实例目录、会话退出码或并发隔离。将 .app 启动作为内部引擎候选评估；普通界面仍只提供一个启动按钮，不把“外部打开/受控启动”再次变成必须选择的方案，也不能悄悄降级实例启动。
3. **不为此自动改写上游应用包。** 不默认复制本体到 MacOS、重写 plist 或重新签名。对于原始 H/I 脚本包，优先评估不改包的受控本体适配；如改用 NSWorkspace，必须先验证该包确实传递参数并提供可跟踪的实际游戏生命周期。
4. **暂不增加新启动路径或放宽并发白名单。** 下一步只做最小的“原包直接打开”受控实验，观测实际 argv、实例 user/config/save 生效与真实进程身份，再决定是否支持托管启动。系统能打开第二个窗口与游戏能够独立保存是两项验收。

现有后端复杂度有保护存档和进程归属的必要部分；公开多个实验方案、让用户手选与识别结果可能不一致的版本，则属于可移除的产品复杂度。相关历史参数、缓存与样本边界见 [固定源码契约](cdda-real-game-acceptance-contract.md)。

## 后续本地 API 验证

[NSWorkspace 实验记录](../quality/nsworkspace-launch-probe-2026-10-29.md)补充了真实系统调用：AppKit 合成接收器可取得匹配 PID，脚本分支仍丢参数；两个原生测试实例同时运行。已调整原生入口的 I QA 包保留时间戳复制后进入主菜单并退出，退出后多余窗口查询触发系统提示。该结果不等于原始发行包通过，也不包含存档绑定或真实多版本并发认证。
