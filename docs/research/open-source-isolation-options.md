# 多版本实例隔离与备份：开源方案评估

核查日期：2026-10-28。范围：当前共享安装缓存、完整写入边界、进程生命周期、工作区锁偶发失败及实例备份。只读取官方项目文档、源码、声明和许可证；未安装、执行或集成下列新增候选。链接中的 main/master/develop 是本次读取入口，正式引入时仍须固定提交、制品摘要及依赖清单。

当前基线为 Rust/Tauri、schema 8、实例目录登记、物理目录预检与持久状态派生的全局占用，绑定仍未验证。相关项目内边界见 [资源占用](../design/resource-ownership.md)、[实例预检](../design/instance-resource-conflicts.md)、[CDDA 实包证据](cdda-real-game-acceptance-contract.md)。这些事实独立于下列候选的功能宣传。

## 结论与采用次序

存在可复用的组件和成熟的实例设计；本次未找到能直接接管 CDDA、多平台图形游戏、完整写入隔离、未知会话恢复和存档一致性验收的单一现成方案。应按能力组合，保持现有未验证配置的全局保护。

| 优先级 | 能力 | 建议 | 当前能否直接宣称问题已解决 |
| --- | --- | --- | --- |
| P0 | Hub 自身安全文件访问、物理目录比较 | 继续使用已依赖的 cap-std/cap-fs-ext、same-file | 只覆盖调用这些 API 的管理器代码，不约束第三方游戏 |
| P0 | 工作区锁失败 | 先复现与定位；fs4/标准库锁作为差异实验候选 | 不能通过更换包名认定修复，底层内核锁语义仍须验证 |
| P1 | 实例目录、配置与生命周期设计 | 参考 Prism Launcher；参考 Catapult 的 CDDA 版本/更新布局 | 不能直接把 Minecraft 或其他启动器的支持声明转成 CDDA 绑定证据 |
| P1 | Linux 执行期写入限制 | 对 bubblewrap 做受控图形样本实验 | 需要自己定义挂载、设备、显示/音频、子进程和失败策略 |
| P1 | macOS 当前交付路径 | 先验证每实例独立完整安装副本与独立玩家目录，再评估平台沙箱 | 复制本身不限制 HOME、临时目录或其他外部写入；不能立即开放安全并发 |
| P2 | 增量与异地备份 | 将 restic 评估为可选外部后端 | 不替代当前独立归档恢复契约、停写/冻结协议或游戏加载验证 |

此优先级是结合本项目现状作出的工程判断，不是上游产品的兼容性承诺。当前不建议替换整套技术栈，也不因引入新工具撤销全局保护。

## 实例与 CDDA 启动器

**Prism Launcher：实例模型参考。** 官方仓库面向 Minecraft 多安装管理；源码 `InstanceList` 维护独立实例根，扫描实例、处理实例关联及规范化路径，适合参考 Installation 与 PlayInstance 的分层、每实例设置和进程/界面状态设计。它采用 C++/Qt，不能作为 Rust/Tauri 的即插即用 CDDA 调度库。本文没有从其多实例能力推断第三方游戏的文件沙箱能力。项目主代码为 GPL-3.0-only，COPYING 还列出继承代码及其他组成部分；借鉴设计与直接复制代码须分开评估。来源：[官方仓库](https://github.com/PrismLauncher/PrismLauncher)、[实例列表源码](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/InstanceList.cpp)、[COPYING](https://github.com/PrismLauncher/PrismLauncher/blob/develop/COPYING.md)。

**Catapult：CDDA 布局和升级流程参考。** 官方说明支持 DDA/BN、多个游戏版本安装与切换、保留玩家数据的更新、存档备份；系统要求列出 Windows 7+ 或 Linux，没有列出 macOS。该项目的 MIT 许可和面向 CDDA 的实现使它成为更直接的代码考察对象，但 README 的“多个版本”不等于已经证明同时运行时写入隔离，更不证明当前官方 H/I-1 macOS 包的 SDL/签名/缓存行为。建议先针对下载布局、用户数据目录识别和更新保留逻辑逐文件评估；本轮未逐行审计其执行路径。来源：[README](https://github.com/qrrk/Catapult/blob/master/README.md)、[MIT 许可证](https://github.com/qrrk/Catapult/blob/master/LICENSE)。

**原始 CDDA Game Launcher：历史参考，不作为新核心依赖。** 原项目明确已停止维护，列出版本/构建检测、保留修改的升级及自动备份，并描述 `previous_version` 搬移方案。可用作旧布局与数据迁移案例，不能把“旧目录保留”当成已验证、独立介质可恢复的备份契约，也不能代替 Hub 的更新 journal 和回滚验收。原项目为 MIT；本轮没有核查其推荐分支的当前维护质量。来源：[官方 README](https://github.com/remyroy/CDDA-Game-Launcher/blob/master/README.md)、[许可证](https://github.com/remyroy/CDDA-Game-Launcher/blob/master/LICENSE)。

## 文件访问和锁

**cap-std/cap-fs-ext、same-file：直接复用，且已经使用。** cap-std 通过目录能力句柄进行相对文件访问，提供防路径穿越的基础；same-file 用于判断不同路径是否对应同一文件，不能单独推出整个目录树稳定或完整写入范围。继续让管理器自己的复制、建目录、归档和恢复操作使用这些边界，比再引入字符串前缀判断更合适。推论边界：普通外部游戏并不会因为启动它的 Hub 使用 cap-std 就自动只能访问这些句柄。cap-std 包声明 `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`；same-file 1.0.6 声明 Unlicense/MIT。来源：[cap-std 设计](https://github.com/sunfishcode/cap-std/blob/main/README.md)、[cap-std 包声明](https://github.com/sunfishcode/cap-std/blob/main/cap-std/Cargo.toml)、[same-file 包声明](https://github.com/BurntSushi/same-file/blob/master/Cargo.toml)。

**fs4：候选实现，不是已知修复。** 它从 fs2 派生，提供同步/异步文件工具并改用 rustix，包声明为 MIT OR Apache-2.0。可以用固定版本做并列复现实验，比较 try-lock、显式释放、句柄复制/继承和重开时序；没有证据前不应迁移并宣布偶发失败修复。现有 fs2 文档特别指出跨平台锁语义、重复句柄注意事项，且文件锁仅可依赖为协作式锁；它不是防止未配合游戏写入的强制目录锁。来源：[fs4 README](https://github.com/al8n/fs4/blob/main/README.md)、[fs4 包声明](https://github.com/al8n/fs4/blob/main/Cargo.toml)、[fs2 锁语义](https://docs.rs/fs2/latest/fs2/trait.FileExt.html#notes-on-file-locks)。本次资料不能定位本仓库那一次 WORKSPACE_LOCKED 的根因。

## 执行期限制与平台差异

**bubblewrap：Linux 首选实验候选。** 它构造 mount/user 等 Linux namespace 沙箱，可把安装/运行时目录只读映射、把实例目录单独设为可写，并可使用 PID namespace 处理派生进程视图。上游明确说工具本身不是配好安全策略的完整沙箱，保护效果由调用者参数决定。图形游戏还需要明确显示、音频、GPU、设备和 IPC 访问；不能把开放宿主显示会话等同于完整安全边界。当前上游需要可用的非特权用户 namespace；不能套用于 macOS/Windows。建议先用新测试实例验证拒绝越界写入和图形存档重载，工具不可用时保持保守阻止，不静默降级后宣称隔离成功。仓库 COPYING 为 LGPL 2.1 文本，具体源文件声明还须随固定版本核查。来源：[官方 README 与限制](https://github.com/containers/bubblewrap/blob/main/README.md)、[COPYING](https://github.com/containers/bubblewrap/blob/main/COPYING)。

**Windows：Job Object 与文件隔离分开。** 微软 Job Object 能将进程作为一组管理，文档也说明子进程加入和 breakaway 例外；它适合生命周期适配器，但不是目录写入白名单。需要另行设计文件访问隔离和进程接管/退出证据。来源：[微软 Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)。

**Sandboxie 只能有条件考察。** 官方项目面向 Windows，提供多个沙箱及文件/注册表隔离，涉及驱动和服务，部署成本明显高于纯 Rust 库。尤其不能把整个仓库笼统标成“GPL 可直接集成”：Classic/核心有 GPLv3，而 Plus/SandMan UI 在当前 LICENSE.Plus 中使用自定义许可，包含商业使用与自编译分发限制，QSbieAPI 等组成部分又有各自许可证。因此不列为当前企业交付的默认开源依赖；如需实验，应先限定确切组件和制品。来源：[官方 README](https://github.com/sandboxie-plus/Sandboxie)、[LICENSE.Classic](https://github.com/sandboxie-plus/Sandboxie/blob/master/LICENSE.Classic)、[LICENSE.Plus 及组成部分](https://github.com/sandboxie-plus/Sandboxie/blob/master/LICENSE.Plus)。

**macOS：没有从上述候选获得跨平台捷径。** Apple 的 App Sandbox 文档描述子进程继承、entitlement 与 XPC 分权，这属于平台能力，不是可以原样替换 bubblewrap 的开源启动库；文档中的签名 helper 模型也不能直接推断任意已下载游戏均可按同一方式封装。当前应先保持样本级验证，明确独立安装副本、玩家目录、HOME/临时目录、动态库路径及包完整性，再评估受控执行器。此前测试用 sandbox-exec 的结果不自动构成长期产品支持承诺。来源：[Apple 官方沙箱继承文档](https://developer-mdn.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html)。

## 实例备份

**restic：可选后端候选。** 官方项目支持 Linux/macOS/Windows、去重、加密、校验与恢复，BSD-2-Clause；适合后续大量实例快照或外部存储需求。建议通过固定版本 CLI 和受限参数构造独立后端契约，记录实例身份/源范围与快照 ID，先验证取消、恢复目标、校验、离线介质及密码恢复流程。不要把 restic 仓库直接写成现有独立 tar.zst 的同一格式，也不要无迁移计划替换已交付的恢复路径。来源：[官方介绍与设计](https://github.com/restic/restic/blob/master/README.md)、[BSD-2-Clause](https://github.com/restic/restic/blob/master/LICENSE)、[备份/VSS 文档](https://restic.readthedocs.io/en/stable/040_backup.html)。

工程推论：后端能完整恢复所读字节，不等于这些字节来自游戏同一时刻，也不等于跨游戏版本兼容；Hub 仍须负责范围绑定、采集稳定性、运行中冲突、保护点和游戏加载验证。Windows VSS 是平台快照能力，不应被扩写成三平台自动获得游戏语义一致性。

## 接入前的最小实验

1. 为锁失败保留可复现触发、真实 OS 错误和句柄生命周期证据；不同锁实现只改变一个变量做对照。
2. 从完整新安装副本起步，禁止硬链接共享仍可能写入的文件；记录程序、资源和缓存边界。独立玩家目录配合完整副本能减少共享，但未证明没有其他系统写入。
3. Linux 独立评估 bubblewrap 的越界写入拒绝、图形/声音启动、保存重载、子进程存活及 Hub 重启；其他平台保留独立适配器门。
4. 接入实例备份范围与冲突调度，完成 Hub 内双实例真实游玩、交叉保存、单实例退出、未知会话与同范围拒绝后，再提升对应样本能力。
5. 每个实际采用的版本记录许可证/NOTICE、依赖及制品来源；命令行集成与复制代码分别核对。本报告只记录上游声明，不作许可证兼容结论。

## 本轮后续诊断结果

上面的 fs4 建议属于调研时的候选评估。随后主实施已复现共享描述符下仅关闭句柄不足以释放锁的生命周期缺口，并改为私有 guard 显式解锁，未替换 fs2。原始竞争未捕获每个内核事件，修复范围与压力结果单独见 [工作区锁诊断](../quality/workspace-lock-lifecycle-diagnosis.md)；这个结论不提升第三方游戏隔离能力。
