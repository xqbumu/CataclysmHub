# 开源与公开源码启动器的 macOS 启动及授权方案调研

日期：2026-10-30。只读联网及源码调查；未运行这些启动器或游戏，未修改签名、隔离属性、系统设置或产品代码。结论为固定提交源码事实，不代表本机正常授权验收。

## 主要结论

**有可复用的工程方案，但没有找到可直接接入、同时保留原隔离属性的通用正常授权实现。** 建议保留 Hub 当前受控本体启动和实例目录，后续只评估一个明确的“当前安装信任处理”动作。主动移除 quarantine 必须按真实含义向用户说明，并取得单独确认，不能称为正常 Gatekeeper 授权，也不由本次研究授权执行。

| 项目 | 启动及信任处理的源码事实 | 对 Hub 的适用结论 |
|---|---|---|
| TLG Launcher | Resources 本体 + userdir；安装时主动移除游戏 quarantine | 最贴近当前问题；可参考结构，不能照搬静默清理和共享用户目录 |
| CatLauncher | 实际运行包内脚本；DMG 后用 ditto；去隔离说明针对启动器自身 | 复制方法可验证；脚本、共享数据与删除旧版不适用 |
| Catapult 原版 / Dabdoob | 原版无该提交的 macOS 启动分支；fork 使用 open --args | 不证明实际游戏 PID 或目录参数生效 |
| Prism Launcher | 每实例游戏根、直接 Java 子进程；自身具备条件式签名/公证流水线 | 可参考生命周期及发行流程；不是第三方 CDDA 包授权方案 |
| Heroic | 审阅的原生侧载分支直接 spawn；README 的 no-quarantine 用于 Heroic 自身 | 可参考参数、cwd/env 和日志，不能推断替游戏完成授权 |
| itch.io / butler / smaug | .app 使用 open -W --args；上报 open PID，额外按路径找进程 | 系统打开实现可读，但进程语义及 CDDA 包装脚本不满足当前保护要求 |

以上事实的固定提交、行号与适用边界见下文。GitHub 搜索只作为定位手段，结论以实际文件为准；搜索未命中不证明整个项目绝无相关功能。

同生态中，最贴近当前问题的 TLG 原生 macOS 启动器采用“直接启动 Resources 本体 + 独立 userdir”，同时在安装游戏时主动移除 quarantine；它没有提供“保留隔离属性、正常系统授权且继续受控实例启动”的现成解法。另两个宣称支持 macOS 的启动器分别存在脚本传参和进程监测适用性问题。不能把“别人能启动”当成正常授权链路成立，也不应据此推翻 Hub 已实测的独立存档和保护机制。

## 1. TLG Launcher：明确绕开包装脚本，并主动去隔离

固定提交：`ad0916c40f3f448c64f47dea11b67c9d131b27f2`。

- GameLauncher 明确记录包装脚本不转发参数，选择 `Contents/Resources/cataclysm-tlg-tiles`；构造带尾随斜杠的 `--userdir`、Resources cwd、DYLD 环境，使用 Foundation Process 直接启动，不使用 open/NSWorkspace。[实际计划与运行](https://github.com/jalexspringer/tlg-launcher/blob/ad0916c40f3f448c64f47dea11b67c9d131b27f2/Sources/TLGLauncherCore/GameLauncher.swift#L3-L65)
- 安装器先按 GitHub digest 核验下载，挂载 DMG，FileManager.copyItem 到 staging、验证包后，明确调用 `/usr/bin/xattr -dr com.apple.quarantine <staged>`，且忽略该步骤错误。这是**主动移除游戏包隔离属性**，不是因为文件恰好没有带 quarantine，也不是正常用户授权 API。[安装路径](https://github.com/jalexspringer/tlg-launcher/blob/ad0916c40f3f448c64f47dea11b67c9d131b27f2/Sources/TLGLauncherCore/UpdateInstaller.swift#L117-L167)
- 游戏用户目录固定为 Application Support/Cataclysm-TLG；多版本安装目录和一个用户目录分离，不能据此证明每个版本/实例独立写入。[路径](https://github.com/jalexspringer/tlg-launcher/blob/ad0916c40f3f448c64f47dea11b67c9d131b27f2/Sources/TLGLauncherCore/LauncherPaths.swift#L19-L43)
- README 的 Open Anyway/xattr 说明针对 **TLG Launcher 自身**；构建脚本仅 ad-hoc 签名启动器，不等于游戏经过 Developer ID 公证。[安装说明](https://github.com/jalexspringer/tlg-launcher/blob/ad0916c40f3f448c64f47dea11b67c9d131b27f2/README.md#L32-L45)、[构建签名](https://github.com/jalexspringer/tlg-launcher/blob/ad0916c40f3f448c64f47dea11b67c9d131b27f2/Scripts/make-app.sh#L89-L92)
- 许可证文件为 CC BY-SA 4.0；可研究思路，直接复制前须核对项目的许可要求。[许可证](https://github.com/jalexspringer/tlg-launcher/blob/ad0916c40f3f448c64f47dea11b67c9d131b27f2/LICENSE)

适用性：受控本体、固定 cwd/env、带尾随斜杠的 userdir 与 Hub 当前方向一致；不能直接复用自动 xattr 清理，也不能继承共享 userdir 或忽略授权步骤失败的语义。缓存核验可参考，但不是 Gatekeeper 修复。

## 2. CatLauncher：macOS 实际运行 Cataclysm.sh

固定提交：`d6693eb82ae48cb95a0ee3a19602cb354d381003`。

- macOS 实际路径组合为 Cataclysm.app/Contents/MacOS/Cataclysm.sh；prepare_launch 直接 Command::new 该文件、设置父目录 cwd、添加 --userdir，而非打开整个 .app。[路径选择](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/cat-launcher/src-tauri/src/filesystem/paths.rs#L131-L137)、[文件名](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/cat-launcher/src-tauri/src/filesystem/paths.rs#L183-L198)、[启动](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/cat-launcher/src-tauri/src/launch_game/launch_game.rs#L146-L174)
- 仓库搜索未找到对 Cataclysm.sh 补充参数转发的代码；这只是调查范围内的负面证据，不能保证所有下载版本都相同。对已知吞参数的 H/I 包，此调用本身不能证明实例目录生效。
- UserData 按游戏 variant 分配；安装后还会删除同级其他安装版本，不符合 Hub 保留多个游戏版本及独立实例的要求。[用户目录](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/cat-launcher/src-tauri/src/filesystem/paths.rs#L338-L345)、[删除旧安装](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/cat-launcher/src-tauri/src/install_release/install_release.rs#L115-L153)
- DMG 由 dmg::Attach 挂载，macOS 文件复制使用 ditto；这里没有发现主动清理 quarantine 的调用，但**不能据此断言下载结果必然无隔离或正常授权**，需实际核对来源属性。[DMG 处理](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/cat-launcher/src-tauri/src/infra/archive.rs#L84-L92)、[ditto](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/cat-launcher/src-tauri/src/filesystem/utils.rs#L22-L38)
- README 中的去隔离/关闭 Gatekeeper 说明针对启动器 DMG，不是上述游戏的授权实现。README 声明许可证为 CC BY-NC-ND 4.0，不能作为可直接搬用到商业产品的普通开源代码来源。[说明及许可](https://github.com/abhi-kr-2100/CatLauncher/blob/d6693eb82ae48cb95a0ee3a19602cb354d381003/README.md#L33-L57)

适用性：可借鉴子进程 stdout/stderr 诊断和 ditto 的包复制方案；不可照搬脚本启动、共享用户目录、删除历史版本及关闭 Gatekeeper 的建议。ditto 只解决复制，不解决授权。

## 3. Catapult 与 Dabdoob fork：名称和 macOS 实现不可混淆

Catapult 原版固定提交：`f90f1cf89ee285f1624baab47e5cc916b727a364`。其当前 _start_game 只有 Linux/Windows 分支，其他系统直接返回，导出预设也只有 Linux/Windows。因此当前原版不能作为 macOS 授权方案参考。[启动分支](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/scripts/Catapult.gd#L393-L415)、[预设](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/export_presets.cfg#L1-L61)。许可证为 [MIT](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/LICENSE)。不据此否定其他历史版本。

Dabdoob fork 固定提交：`0ebda116fd2af95d90ae309605ef088f075f5236`。

- macOS 区分直接程序和 .app；直接程序分支用 bash -c 切 cwd 执行；.app 分支通过 open <bundle> --args --userdir ... 启动，没有 -W 或 -n。[分支](https://github.com/Hihahahalol/Catapult_Dabdoob/blob/0ebda116fd2af95d90ae309605ef088f075f5236/scripts/Catapult.gd#L834-L885)、[具体执行](https://github.com/Hihahahalol/Catapult_Dabdoob/blob/0ebda116fd2af95d90ae309605ef088f075f5236/scripts/Catapult.gd#L1711-L1778)
- 代码声称监测 .app，但实际交给监测器的是 open 工具。这里不能把 open 返回、包装脚本或 app 可见等同于真实游戏生命周期；已知 H/I 包吞参数的问题仍存在。不可替换 Hub 的直接子进程监测和失败保护。
- README 的 macOS beta 建议是针对 Dabdoob 本身解除或关闭 Gatekeeper；不是保留系统检查的游戏授权实现。[README](https://github.com/Hihahahalol/Catapult_Dabdoob/blob/0ebda116fd2af95d90ae309605ef088f075f5236/README.md#L40-L42)
- DMG 只读挂载、复制并 detach，可参考任务结构，但复制函数会将某些失败降为非关键日志，不能继承为 Hub 完整安装验证语义。[DMG](https://github.com/Hihahahalol/Catapult_Dabdoob/blob/0ebda116fd2af95d90ae309605ef088f075f5236/scripts/filesystem_helper.gd#L398-L463)、[复制](https://github.com/Hihahahalol/Catapult_Dabdoob/blob/0ebda116fd2af95d90ae309605ef088f075f5236/scripts/filesystem_helper.gd#L75-L99)。许可证为 [MIT](https://github.com/Hihahahalol/Catapult_Dabdoob/blob/0ebda116fd2af95d90ae309605ef088f075f5236/LICENSE)。

## 4. Bright Nights 官方脚本：通用包装器与 .app 包装器并不等价

固定提交：`b4a6341b169b1e45f479134fe7f92d1986201b3c`。

仓库根 cataclysm-launcher 以 exec ./$BIN "$@" 转发参数，但 build-data/osx/Cataclysm.sh 的图形分支仍只调用 ./cataclysm-bn-tiles，不转发参数，终端分支交给 Terminal。因此不能看到通用包装器支持 "$@"，就认定 macOS .app 已支持目录隔离。[通用包装器](https://github.com/cataclysmbn/Cataclysm-BN/blob/b4a6341b169b1e45f479134fe7f92d1986201b3c/cataclysm-launcher#L36-L51)、[macOS 包装器](https://github.com/cataclysmbn/Cataclysm-BN/blob/b4a6341b169b1e45f479134fe7f92d1986201b3c/build-data/osx/Cataclysm.sh#L16-L22)。未把这些源码当作每个已发布资产实际内容的保证。

## 5. Prism Launcher：复用实例及发行实践，不把 Java 当成原生游戏授权样本

固定提交：`573665ffa1ccbd7e19df39c28193d26d5b202d60`。

- gameRoot 从 instanceRoot 下的 minecraft/.minecraft 派生，保存目录是其 saves；游戏参数的 game_directory 指向该根。启动步骤使用实例环境和 cwd，并通过 LoggedProcess 启动 Java、记录真实子进程 PID。[目录](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/launcher/minecraft/MinecraftInstance.cpp#L370-L378)、[存档](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/launcher/minecraft/MinecraftInstance.cpp#L462-L465)、[目录参数](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/launcher/minecraft/MinecraftInstance.cpp#L1067-L1077)、[cwd](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/launcher/minecraft/MinecraftInstance.cpp#L1252-L1258)、[子进程启动](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/launcher/minecraft/launch/LauncherPartLaunch.cpp#L99-L148)、[PID](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/launcher/minecraft/launch/LauncherPartLaunch.cpp#L198-L205)。
- macOS 打包工作流有证书签名分支；证书缺失时退回 ad-hoc。公证凭据存在才运行 notarytool 和 stapler，缺失则跳过。这是 **Prism 自身**的发行流程；不能据此宣称所有构建都公证，更不能使下载的任意游戏获得 Apple 信任。[条件式签名、公证](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/.github/actions/package/macos/action.yml#L42-L97)。
- Minecraft 主要由 Java 加载代码与数据，不等于执行一个带脚本入口的 CDDA .app。源码能支撑“实例根 + 明确子进程”的架构参考，不能支撑当前 Gatekeeper 问题已被解决。许可证为 [GPLv3 文本](https://github.com/PrismLauncher/PrismLauncher/blob/573665ffa1ccbd7e19df39c28193d26d5b202d60/LICENSE)；本轮只参考设计，没有复制实现。

## 6. Heroic：侧载启动与启动器自身安装要分开

固定提交：`509a34feea8541b73c928e289f2cf262d432db4a`（package.json 版本 2.22.4）。

- 审阅的原生侧载分支组合用户参数、环境与目标文件所在目录，再进入 callRunner；最终直接 spawn，并采集 stdout/stderr。该代码不能证明所有 .app 都能直接作为文件运行，也没有对 CDDA 包装脚本做参数转发适配。[原生侧载](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/blob/509a34feea8541b73c928e289f2cf262d432db4a/src/backend/storeManagers/storeManagerCommon/games.ts#L200-L245)、[spawn 及输出](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/blob/509a34feea8541b73c928e289f2cf262d432db4a/src/backend/launcher.ts#L1730-L1771)。
- README 的 Homebrew --no-quarantine 是安装 Heroic 自身的说明，不是它替第三方游戏提供正常系统授权的证据。[安装说明](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/blob/509a34feea8541b73c928e289f2cf262d432db4a/README.md#L223-L226)。仓库中的 macOS .app 快捷方式又是单独的 run.sh 包装，不能与实际游戏入口混淆。[快捷方式](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/blob/509a34feea8541b73c928e289f2cf262d432db4a/src/backend/shortcuts/shortcuts/shortcuts.ts#L182-L241)。
- 可参考原生参数传递和日志，但不照搬通用脚本、共享配置或状态推断。项目声明 [GPL-3.0-only](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/blob/509a34feea8541b73c928e289f2cf262d432db4a/package.json#L1-L15)。

## 7. itch.io：有现成 open 包启动实现，但不能直接换掉 Hub 生命周期模型

butler 固定提交：`d1416eb2b1d5d411ba8c91ffa5d92683b44ff8c1`；其 go.mod 固定 smaug `c947b35e2c418bdf0671b3a46c9d93a8d308bc73`、ox `1e6be0bfd382a5faacfe3f3e33a7850d595a475d`。[依赖版本](https://github.com/itchio/butler/blob/d1416eb2b1d5d411ba8c91ffa5d92683b44ff8c1/go.mod)。

- butler 将目标、cwd、argv、环境及 OnStart 回调交给 runner。[调用边界](https://github.com/itchio/butler/blob/d1416eb2b1d5d411ba8c91ffa5d92683b44ff8c1/endpoints/launch/launchers/native/native.go#L230-L260)。
- smaug 发现目标在 .app 内时，会提升到整个应用包，再用 open -W <bundle> --args ...，设置 open 工具的 cwd/env。该分支没有 -n；上报的是 cmd.Process.Pid，即 open PID，不是已证明的游戏 PID，等待也是 open 的退出。终止时另用 pgrep -f 匹配包入口路径并比较前后 PID；前置快照失败时退回全部匹配 PID。此语义不符合 Hub 当前“无法确认归属时保守阻断”的要求。[包识别](https://github.com/itchio/smaug/blob/c947b35e2c418bdf0671b3a46c9d93a8d308bc73/runner/mac_launch_target.go#L24-L65)、[系统打开](https://github.com/itchio/smaug/blob/c947b35e2c418bdf0671b3a46c9d93a8d308bc73/runner/app_darwin.go#L73-L109)、[终止与上报](https://github.com/itchio/smaug/blob/c947b35e2c418bdf0671b3a46c9d93a8d308bc73/runner/app_darwin.go#L120-L169)。
- ox 用 NSBundle.executablePath 解析应用入口，并不是替包装脚本补参数。对固定 H/I 包，提升到整个应用仍会走吞参数的脚本。[入口解析](https://github.com/itchio/ox/blob/1e6be0bfd382a5faacfe3f3e33a7850d595a475d/macox/bundles_darwin.go#L11-L27)。
- 可选沙箱分支还会生成 shim .app，调用 sandbox-exec，再交给相同 open 路径。它是隔离执行设计，不是正常 Gatekeeper 授权 API；对 Hub 还引入额外包装、链接和生命周期工作，本轮不建议接入。[shim](https://github.com/itchio/smaug/blob/c947b35e2c418bdf0671b3a46c9d93a8d308bc73/runner/sandboxexec_darwin.go#L224-L301)。审阅的这些 runner 文件没有出现主动去隔离调用，但不据此断言 itch 全部安装流程都不会改变隔离。
- smaug、ox 为 [MIT](https://github.com/itchio/smaug/blob/c947b35e2c418bdf0671b3a46c9d93a8d308bc73/LICENSE)、[MIT](https://github.com/itchio/ox/blob/1e6be0bfd382a5faacfe3f3e33a7850d595a475d/LICENSE)。许可较易复用不代表行为符合当前存档保护契约；本轮未引入依赖或复制源码。

## 给 Hub 的最小方案建议（推论，未实施）

### 保持现有启动方式

继续使用已识别的游戏本体、固定 argv/cwd/env、直接子进程和已有资源门禁。独立 save/config/user 根、程序摘要及目录身份检查、未知会话阻断、只恢复到新目录均保持。不要把启动器自身公证当成第三方游戏授权，也不要为了取得 .app 窗口而改写上游入口或切换到通用 open。

### 将后续工作收敛为安装级信任处理

TLG 证明了常见可用路径是**安装时主动去隔离**，它并未证明正常系统授权已成功。对已经有可信来源、确切构建摘要的已知游戏，可以设计一个默认不执行、用户单独确认的“仅移除此安装的隔离属性”动作：

1. 失败说明中明确这是安全属性变更，展示当前安装的真实完整路径、来源与摘要。它属于整个安装，不属于单个游玩实例；不推广成自动授权其他包、以后版本或任意导入文件。
2. 复用现有全安装及实例的资源门禁；所有相关游戏/任务静止、未知会话已处理后才允许执行。确认时重新核对包身份、程序摘要及目录边界，拒绝变更、外部链接和不明确范围。
3. 仅处理所确认安装范围的 com.apple.quarantine，记录并保留变更前属性供审计/恢复；不清除其他属性、不改签名、不关闭全局 Gatekeeper。失败不显示成功，不自动反复尝试。不能直接照搬忽略 xattr 错误的实现。
4. 完成后仍走原启动计划、原独立目录与进程监测，不自动启动或升级 binding_verified。没有可验证来源、签名异常原因不明或系统明确恶意软件阻断时，不提供通用“修复一切”操作。

这是一条需要单独产品及安全确认的可选工程路线，**本次联网研究没有授权实施或执行该动作**。若要求始终保留原包的全部 quarantine 属性，本轮研究仍未找到可直接接入的现成方案；应继续验证确切发行包的正常系统批准，或取得适当签名、公证及转发参数的上游包。

### 只做一个小验证切片

下一轮先验证保留来源属性的包复制（如 ditto 的 qtn 行为），区分下载时没有属性、复制丢失属性与主动移除。复制方式不能作为绕过安全检查的替代。然后再在新 QA 安装验证显式信任动作的取消、范围变更拒绝、失败记录及字节保持；涉及实际隔离变更时在操作前取得具体确认。成功后才复用已经完成的“保存 → 备份 → 新目录恢复 → 角色重载”闭环验收。

不新增通用启动引擎、Go 服务、默认重签名、脚本补丁、多套公开启动模式或复杂全局配置。研究源码没有改变当前代码支持范围，正式多版本并发及跨平台认证继续保持未验收状态。

## 本轮交付边界

本轮只联网读取官方仓库和固定源码，未运行第三方启动器、未修改生产代码、未变更系统或游戏的隔离/签名，也未访问玩家存档。没有理由重跑游戏、前后端测试或打包；本报告提供的是源码证据与待实施建议，不是 Gatekeeper 修复验收通过。
