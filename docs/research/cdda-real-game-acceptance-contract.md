# 官方 macOS CDDA 样本：绑定与并发验收契约

日期：2026-10-28。状态：验收设计及固定源码研究。研究代理不执行游戏；主实施代理提供的官方样本 `--version` 结果单独列于下节，不等于绑定或游玩验收通过。实际运行结果应另存样本清单与验证记录，不将此文件视为支持声明。

## 精确发布身份

官方 Git refs API 返回的是 annotated tag，需解引用 tag object 才得到提交；release 的 `target_commitish` 可能是 `master`，不能当作精确源码身份。

| 发布标签 | tag object | 精确 commit | 先前研究的关系 |
|---|---|---|---|
| 0.H-RELEASE | ca615b07eadad8583fdfaf13ab9197f3d757eef3 | 08f04fd07f028219a65f2f1fc8e73719dc9c954e | 不是 `0.H` 标签的 5d069f2eaccd125d22b53617ba73ce7fab157a9f；需要重新核对 |
| 0.I-1 | 0fb5d5b67811b68ad15798cda13d2b5ec07526cd | 7b2efa5cea38e4d4d97dd0e63b28b9148623da59 | release 元数据的 target_commitish 为 master，已另行解析 tag |

来源：[0.H-RELEASE ref](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/git/ref/tags/0.H-RELEASE)、[H tag object](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/git/tags/ca615b07eadad8583fdfaf13ab9197f3d757eef3)、[0.I-1 ref](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/git/ref/tags/0.I-1)、[I tag object](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/git/tags/0fb5d5b67811b68ad15798cda13d2b5ec07526cd)。API 于上述日期读取；解析结果应与下载时保存的原始 JSON 一并保留。

真实图形样本候选为官方资产 `cdda-osx-with-graphics-universal-2024-11-23-1857.dmg` 和 `cdda-osx-with-graphics-universal-2026-09-19-2324.dmg`。本次取得的官方 release JSON 中，H 资产没有 digest；I 资产提供 SHA-256 `8c169974d93762d1bedffee75c6847eb70a1d5b05e6448c1b829e314ba8bfaf6`。H 必须明确“仅自算摘要、上游摘要缺失”；不能把自算摘要写成上游真实性验证。I 也需要实际下载后独立计算再比较，本文未执行下载校验。[H 发布元数据](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/releases/tags/0.H-RELEASE)、[I 发布元数据](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/releases/tags/0.I-1)。

每个样本至少记录：发布/资产 URL、资产名及 ID、下载时间和大小、自算 DMG 摘要、上游 digest 是否存在与比较结果、解包布局、实际程序摘要及 Mach-O 架构、资源清单摘要、签名/权限观测、构建/版本声明及其来源。精确源码提交与本地二进制之间仍需可信发布链及实测关联，不能只靠同名。

## 实包版本输出补充：发布标签与编译标识分开记录

主实施代理已从上述官方发布取得样本，并报告直接启动 `Contents/Resources/cataclysm-tiles`、配置随包 `DYLD_FRAMEWORK_PATH` / `DYLD_LIBRARY_PATH` 后取得下列 `--version` 首行。研究代理只读取已保存输出并复算本地样本摘要，没有代为执行游戏：

| 资产/程序观测 | 结果 | 证据层级 |
|---|---|---|
| 官方 H DMG 自算 SHA-256 | c8045731b8227d21e88c4aae9a1ec7da7ca532eca4212625ecc1dab291fe5728 | 本地下载件内容身份，上游 digest 缺失 |
| H `cataclysm-tiles` 自算 SHA-256 | ba92c296d23758eab4fe4a74ee5ee624f01edaac051c3777c3c138cac45de21d | 本地程序内容身份 |
| H 程序版本首行 | `Cataclysm Dark Days Ahead: 78fd6e3` | 程序自报告的编译标识，不是发布 tag commit |
| I 程序版本首行 | `Cataclysm Dark Days Ahead: cdda-0.I-2026-09-19-2324 7b2efa5` | 程序自报告，短 SHA 与上述 I tag commit 前缀一致 |
| I `cataclysm-tiles` 自算 SHA-256 | 3f37ff041654edbb88773666344be0bf8eba67cb44d71c15908aeb961d4aac3a | 本地程序内容身份，不能代替实际行为验收 |

主实施原始输出保存在测试目录的 `0.H-RELEASE-version.stdout` 与 `0.I-1-version.stdout`；最终验证记录应归档这些原始输出和运行参数/环境，不只保留人工摘录。输出里的默认 data/user 路径不应解释为实例路径已应用。

官方 commits API 将 `78fd6e3` 解析为完整提交 **78fd6e3af9fd75b2e6f0b1c603ce34c7ab222610**。H 官方发布标签指向 `08f04fd...`，实包程序报告 `78fd6e3`，两者确实不同；应原样保留这两个来源不同的事实，不自动改写其一，也不据此断言伪造、损坏或已知原因。[官方提交解析](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/commits/78fd6e3)、[H 发布 tag object](https://api.github.com/repos/CleverRaven/Cataclysm-DDA/git/tags/ca615b07eadad8583fdfaf13ab9197f3d757eef3)。

研究代理重新获取 `78fd6e3...` 的四份关键源码并与 `08f04fd...` 逐字节比较，`src/main.cpp`、`src/path_info.cpp`、`src/json_loader.cpp`、`src/flexbuffer_cache.cpp` **完全相同**。因此本文针对 H 的参数两遍顺序、默认目录及五类缓存结论在这四份已查文件中没有变化；它不证明全部源码、编译选项、资源或二进制等价。[实包所报提交的参数实现](https://github.com/CleverRaven/Cataclysm-DDA/blob/78fd6e3af9fd75b2e6f0b1c603ce34c7ab222610/src/main.cpp#L345-L548)、[目录实现](https://github.com/CleverRaven/Cataclysm-DDA/blob/78fd6e3af9fd75b2e6f0b1c603ce34c7ab222610/src/path_info.cpp#L105-L168)、[缓存根](https://github.com/CleverRaven/Cataclysm-DDA/blob/78fd6e3af9fd75b2e6f0b1c603ce34c7ab222610/src/json_loader.cpp#L15-L42)、[缓存实现](https://github.com/CleverRaven/Cataclysm-DDA/blob/78fd6e3af9fd75b2e6f0b1c603ce34c7ab222610/src/flexbuffer_cache.cpp#L220-L364)。

`--version` 仍在两遍目录参数处理之前退出，因此这些真实输出仅补充了样本身份观测；它们没有验证任何实例目录绑定、生效参数、配置/缓存隔离或多版本实际游玩。[实包所报提交的提前退出顺序](https://github.com/CleverRaven/Cataclysm-DDA/blob/78fd6e3af9fd75b2e6f0b1c603ce34c7ab222610/src/main.cpp#L531-L548)。

## 两版路径控制与变化

H-RELEASE 和 I-1 均先初始化默认路径，再解析参数。`--basepath`、`--userdir` 在第一遍处理并重建默认路径；`--datadir`、`--savedir`、`--configdir`、`--memorialdir` 在第二遍覆盖。`--version` 在两遍之前提前退出，不能用其与路径参数组合来证明绑定已生效。[H 参数及两遍调用](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/main.cpp#L345-L548)、[I 参数及两遍调用](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/src/main.cpp#L347-L550)。

默认 save、memorial 从 user 派生，XDG 构建的 config 可以来自 XDG_CONFIG_HOME 或 HOME 而非 user。I-1 新增 user/achievements，说明按“几个已知子目录”列举而不覆盖整个 user 根会漏掉新版本范围；不能只隔离 save。两版仍应明确控制 config，涵盖完整 user 和真实 base/data 写入范围。[H 默认路径](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/path_info.cpp#L105-L168)、[I 默认路径含 achievements](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/src/path_info.cpp#L109-L174)。

两版 JSON 加载器均有 base/cache、config/cache、data/cache、memorial/cache、user/cache 五组磁盘缓存。I-1 改用 cata_path/get_unrelative_path 等路径表达，但未因此取消 base/data 缓存共享写入。配置日志与缓存、资源缓存的实际生成/清理均应纳入观测。源文件只读不等于其所属根没有缓存写入。[H 缓存根](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/json_loader.cpp#L15-L42)、[I 缓存根](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/src/json_loader.cpp#L14-L41)、[H 缓存写入](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/flexbuffer_cache.cpp#L320-L364)。

这些是已确认源码范围，不是 SDL、系统库、所有游戏路径及子进程写入的穷尽证明。候选参数必须是适配器受控绝对路径，额外参数拒绝覆盖路径、共享模式和未知行为；任意不同版本或不同可执行文件路径不构成目录隔离证明。

## macOS bundle 与真正被监测的程序

两版所查源码的 Info.plist 都声明 `CFBundleExecutable=Cataclysm.sh`；`CFBundleShortVersionString=1.0`、`CFBundleVersion=1` 不是游戏发布版本。打包规则把实际程序和 data、gfx、lang、动态库/框架放在 `Contents/Resources`，脚本在 `Contents/MacOS`。[H Info.plist](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/build-data/osx/Info.plist#L5-L25)、[H 打包规则](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/Makefile#L1182-L1244)、[I 打包规则](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/Makefile#L1294-L1330)。

脚本先切换到 Resources。图形版用 `./cataclysm-tiles` 启动而不是 `exec`，未向其转发 `"$@"`；终端版通过 osascript 向 Terminal 另起命令。故把 bundle 脚本直接导入，会面临路径参数未传递和父进程不等于游戏进程的双重问题。本文仅审阅脚本，不执行 osascript。[H 脚本](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/build-data/osx/Cataclysm.sh#L1-L24)、[I 脚本](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/build-data/osx/Cataclysm.sh#L1-L24)。

真实验收应先核对实际 DMG 内布局，然后对 `Contents/Resources/cataclysm-tiles` 本体建立身份、参数及 cwd 计划，检查实际动态库依赖和启动结果；不能仅因源码预期布局就不检查包。直接启动本体仍需证明其没有交接给另一进程，并验证管理器退出、系统终止及 PID 复用的状态处理。

## 非交互测试的用途与局限

`--check-mods` 设置 test_mode，跳过普通 curses 界面初始化，仍初始化/加载选项、静态数据，再调用 check_mod_data，按检查及 debug 错误情况返回退出码。它在正常主菜单、世界加载和回合循环之前退出。该参数的 handler 把它之后的所有参数收集为 mod ID，因此受控目录参数必须放在前面，`--check-mods <明确有效的 mod ID>` 放最后；不能把后续目录参数误当 mod 名。[H handler](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/main.cpp#L320-L335)、[H 检查执行与退出](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/main.cpp#L757-L801)、[I handler](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/src/main.cpp#L312-L327)、[I 检查执行与退出](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/src/main.cpp#L759-L803)。

check_mod_data 会初始化世界生成器、make_new_world，加载核心/依赖/mod 数据并 delete_world。故它可能在存档范围产生临时写入/删除，不能叫“只读版本探测”；退出后目录快照也可能漏掉这些瞬时文件。它适合在纯测试实例内验证数据加载及部分目录解析，不证明正常游玩保存/重载，也不证明所有资源路径都被覆盖。[H 检查实现](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/game.cpp#L466-L558)、[I 检查实现](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/src/game.cpp#L503-L595)。

`--jsonverify` 在静态数据加载后直接退出，尚未进入 check_mod_data 或正常主菜单；其成功更不能证明真实存档绑定。普通模式在随后创建必要目录、打开菜单、发出 game_begin 并运行回合循环，必须单独验收。[H 正常模式](https://github.com/CleverRaven/Cataclysm-DDA/blob/08f04fd07f028219a65f2f1fc8e73719dc9c954e/src/main.cpp#L785-L861)、[I 正常模式](https://github.com/CleverRaven/Cataclysm-DDA/blob/7b2efa5cea38e4d4d97dd0e63b28b9148623da59/src/main.cpp#L787-L878)。

## 真实绑定与并发验收矩阵

所有运行均使用新建测试根、测试用户环境及无玩家原件的安装副本；提前保存源安装与受保护范围的内容摘要/文件类型清单。应记录瞬时写入观测方式和覆盖边界，前后摘要不足以证明“从未写过”。以下均为待执行要求，不是通过结果。

| 阶段 | 必需动作和证据 | 可以支持的结论 | 不能支持的结论 |
|---|---|---|---|
| 身份 | 核对两份资产、程序摘要、架构、实际布局、版本声明、源码提交关系 | 明确验收针对哪些具体样本 | 任意同版本本地程序都相同/可信 |
| 单样本非交互 | 两版本分别在全新实例运行有效 mod 检查，保存原始 argv/cwd/环境、PID/时间/退出码、日志和变化清单 | 相应检查模式可启动并加载指定资源；观测到的配置/缓存/临时世界范围 | 正常菜单、游玩、保存与子进程生命周期已验证 |
| 单样本实际游玩 | 各自进入正常图形菜单、新建测试世界和角色、推进若干回合、改实例设置、游戏内保存退出并重载同一角色 | 该样本在该环境中完成实际存档与配置读写、重载 | 跨版本存档兼容、退出后语义稳定的普遍保证 |
| 双版本真实并发 | 管理器正式实例入口启动 A、B；两个实际角色均处于游戏回合状态且活动时间重叠，分别操作和保存；记录双会话与独立写入证据 | 该两样本/配置的真实并行游玩 | 仅两个窗口或两个检查进程就满足“同时游玩” |
| 退出隔离 | 退出 A，B 继续操作/保存；只完成 A 的会话与后续范围判断，B 仍显示运行 | 监测和会话结论按实例分离 | 退出 A 可将 B 数据判为静默 |
| 冲突负例 | 第三实例分别复用 save、config、base/data 写入根，测试嵌套、符号链接和大小写别名；未知版本保持保守 | 范围冲突不能被不同路径文本或安装名绕过 | 仅登记时拒绝重复根便覆盖运行时路径替换 |
| 任务协调 | A 运行时对其写入范围的备份/恢复拒绝；已验证无关范围的操作可进行，并保存真实范围依据 | 按资源范围协调 | 所有任务继续全局互斥已完成 USR-001 |
| 重启与未知状态 | 游戏仍运行时退出/强制结束管理器并重启；保留相应实例范围保护；确认退出前不清理/覆盖；覆盖 PID 复用/接管用例 | 未知会话保守恢复，保护不会误释放 | 查到某 PID 不存在就证明所有派生进程关闭 |
| 防旁路与回归 | 修改二进制、资源/链接、预览后目录身份、插入重复或未知路径参数；失败时不启动或不释放保护 | 计划与执行间有身份/参数/范围再验证 | 已签名参数仍可永久信任路径现场 |

每项结果应分开记录“执行/未执行/失败/证据不足”，附原始日志或截图与摘要。真实游戏验收不能替代资源租约事务、冲突、取消/恢复和持久化失败的自动化测试；自动化测试也不能替代上述真实游玩。只有明确完成的条目才能提升对应样本能力，不应一次性将安装、存档兼容、备份稳定性与并发支持全部标为已验证。
