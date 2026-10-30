# Gatekeeper 正常授权入口：保留隔离属性的实测

日期：2026-10-30。基线：1b8394d。平台：macOS 27.0.1（26A434），Apple Silicon。

## 结果

**未找到已验证通过的正常系统授权路径，本轮没有修复完成。** 没有修改生产启动逻辑，没有移除隔离属性或重新签名。之前已授权的 QA 副本只用于存档/恢复闭环，不能用于证明本问题已解决。

## 原始包及包级扫描

从挂载的官方 0.I-1 DMG 再创建独立 Game-Quarantined.app，保留时间戳和隔离信息。实际程序摘要仍为 3f37ff041654edbb88773666344be0bf8eba67cb44d71c15908aeb961d4aac3a。程序本体签名检查通过，整个应用包则报 code object is not signed at all；不能混淆两者。

依据 Apple DTS 提及的 gktool，对完整包执行 gktool scan。工具返回 70，明确说明发行签名不满足 Gatekeeper 要求。系统“隐私与安全性”安全区域仍未出现对应 Open Anyway；扫描不是用户授权接口。不能将此结果视为恶意软件判定，也不能仅凭它保证系统提供例外按钮。

## 隔离的原生入口实验

在另一个新 QA 副本 Game-Native-Entry.app 中，只把原字节本体移到 Contents/MacOS，并调整 Info.plist 的 CFBundleExecutable；没有修改原始安装，未重签或清除隔离。它是修改布局的实验样本，不能冒充官方原始发行包。

使用 NSWorkspace.openApplication，明确请求新实例、激活窗口，传入 --version 和独立 user/save/config 参数及 HOME/XDG 环境。--version 按已核对的 I 源码在路径处理前退出。回调完成但 app/PID 缺失、error 为 nil，系统日志明确记录 PID 48240 的 Gatekeeper rejection；设置页仍无例外按钮。回调成功不等于授权、启动或进程监测成功。随后尝试读取应用窗口超时，并产生第二个被拒绝的进程 48260，停止重复查询；没有观察到主菜单或授权对话框。

本实验否定“只移动入口或换成系统 API 就已经修复”的交付结论，不证明所有签名正确的原生包或所有调用上下文都失败。SDK 对 promptsUserIfNeeded 的注释还说明：该开关不控制 Gatekeeper UI，不能把设为 true 当成授权绕过或已验证修复。

## 下载来源记录与失败的复制实验

从 DMG 用命令行复制时，观察到隔离记录的 agent/时间字段为空或为零。Finder 复制中曾观察到 Firefox 下载来源和非零时间戳，因此增加了对正常安装方式的核验；但 Finder 随后报 -36，系统 CopyEngine 日志记录名称无效/过长，最终删除失败的目标。该 Finder 副本不能作为有效完整样本，不能把来源字段差异写成已证明的根因。

从该未完成 Finder 副本再生成的 Finder-Native.app 同样排除，不作为任何成功或失败的授权判据。本轮没有修复 Finder 自身复制问题，也没有把它归因到 Hub。

## 保护及边界

只读查询显示 Gatekeeper assessments enabled，DEP/MDM enrollment 均为 No；这不穷尽所有本地或第三方安全策略。没有更改任何系统设置。

未再次运行官方包装脚本：固定 I 脚本不转发目录参数，不能把原 .app 打开动作作为受控实例启动的替代。未改写 Info.plist/入口到用户安装；布局修改仅限新隔离实验包。此前原实例全部文件的大小、集合和 SHA-256 与备份前基线保持一致，没有创建新的正式游玩会话或更改保护门禁。

## 后续所需证据

当前不应在产品增加未经通过的授权按钮、自动改包或安全属性变更。下一步优先取得由上游提供、入口可转发目录参数、签名与公证完整的实际发行包，重新验证系统授权及 Hub 受控启动；若仍被拒绝，再以该确切包调查系统策略。仅补签本地副本会改变信任身份，也不能当成原发行包正常授权通过。

本轮只有诊断和证据变更，因此没有重跑前后端单元测试或重新打包。实际执行的是签名检查、gktool、NSWorkspace 系统调用、原生设置页观察和原实例摘要对比；验收状态保持未通过。

## 官方依据与证据

- [Apple DTS：Resolving Gatekeeper Problems](https://developer.apple.com/forums/thread/706379)：区分签名、公证和命令行工具的评估问题，并介绍 gktool；对应用批准与内部工具信任的说明不能泛化为任意后台进程已有授权。
- [Apple：NSWorkspace.openApplication](https://developer.apple.com/documentation/appkit/nsworkspace/openapplication(at:configuration:completionhandler:))、[promptsUserIfNeeded](https://developer.apple.com/documentation/appkit/nsworkspace/openconfiguration/promptsuserifneeded)：请求系统打开应用及交互行为，不替代实际结果核对。
- [上游当前构建签名脚本](https://github.com/CleverRaven/Cataclysm-DDA/blob/master/build-scripts/codesign-macos.sh)和 [Info.plist](https://github.com/CleverRaven/Cataclysm-DDA/blob/master/build-data/osx/Info.plist)：源码描述 ad-hoc 签名及脚本入口，不能据此断言任何已发布资产已完成 Developer ID 公证。

结构化记录、扫描原始输出、系统拒绝摘录及一次性 Swift 探针见 evidence/gatekeeper-normal-2026-10-30。未保存游戏原始内容或玩家存档。
