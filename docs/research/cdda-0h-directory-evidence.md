# CDDA 0.H 目录与启动参数证据

日期：2026-10-28。范围：官方 `CleverRaven/Cataclysm-DDA` 的 `0.H` 标签所指提交 `5d069f2eaccd125d22b53617ba73ce7fab157a9f`。本记录仅审阅上游源码与构建脚本，未下载或执行游戏、未测试真实存档、未验证任何本地二进制。它用于设计适配器，不是本地安装已识别、目录绑定已验证或并发可用的证明。

## 结论

独立 `--userdir` 不能单独证明实例完整隔离。配置可能另取 XDG 目录，磁盘 JSON 缓存至少涉及 base、data、config、memorial、user 五个逻辑根。复用同一安装而只隔离用户目录，仍可能共享可写缓存；不同安装能否安全并发还取决于它们实际解析后的 base/data/config 等范围、外部链接和进程行为。不能依据文件夹名称、一个版本侧车或用户勾选来放开并发。

## 启动路径与参数

启动先初始化 base/user 路径及默认文件名，再解析命令行；默认 user 路径受 `USE_HOME_DIR` / `USE_XDG_DIR` 等编译选项影响，某些构建默认 `.`。因此宿主平台和安装目录不足以推定实际用户目录。[启动顺序](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L614-L637)、[平台默认用户目录](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/path_info.cpp#L77-L102)。

| 参数 | 固定提交中的行为 | 适配约束 |
|---|---|---|
| `--basepath` | 第一遍参数，调用 `init_base_path` 后重新计算标准路径 | 可能改变默认 data/gfx/lang 和 base 缓存范围，不是单纯只读资源位置 |
| `--userdir` | 第一遍参数，调用 `init_user_dir` 后重新计算标准路径 | 默认 save、memorial 和多种用户资源在其下；XDG 配置例外 |
| `--datadir` | 第二遍参数，直接调用 `set_datadir` | 控制数据根及其磁盘缓存，还调整 gfx/motd；不能只视为只读 |
| `--savedir` | 第二遍参数，直接调用 `set_savedir` | 覆盖 save 根，不覆盖其他用户写入 |
| `--configdir` | 第二遍参数，直接调用 `set_config_dir` | 覆盖配置根及部分配置文件位置；日志和配置缓存由其派生 |
| `--memorialdir` | 第二遍参数，直接调用 `set_memorialdir` | 覆盖纪念记录根，另有其下缓存 |

来源：[base 参数](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L341-L352)、[user 参数](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L406-L417)、[第二遍目录参数](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L431-L483)、[setter 实现](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/path_info.cpp#L524-L559)。

解析器对第一遍和第二遍各扫描一次参数数组，按各次扫描中的参数顺序应用匹配项。第二遍总在第一遍之后，因此不能仅靠“把管理器参数放最后”来推定所有影响已受控；重复参数和第一遍重设默认值也必须考虑。未知参数被跳过。管理器应由适配器构造受控参数，拒绝路径覆盖、共享模式及未知参数，而非把任意额外参数原样追加。[扫描器](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L246-L277)、[两遍调用](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L538-L544)。

`--optionfile`、`--keymapfile`、`--autopickupfile` 等参数也会改变文件位置；setter 对其中若干值直接赋值，不能因为帮助文案写“在配置目录内”就将任意参数值视为已约束相对路径。目录 setter 对字符串有直接拼接，候选命令必须使用规范化绝对目录及明确分隔符，仍需实际运行验证。[文件参数](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L484-L523)、[原值赋值](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/path_info.cpp#L537-L576)。

## 已知目录派生与写入

默认 save 为 user/save，memorial 为 user/memorial。启用 `USE_XDG_DIR` 时 config 取 `XDG_CONFIG_HOME/cataclysm-dda/`，或 HOME 下 `.config/cataclysm-dda/`，而非 user/config；其他该分支配置默认取 user/config。默认 data 还受 `DATA_DIR_PREFIX`、base 和相对工作目录影响。[标准路径](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/path_info.cpp#L105-L167)。

用户字体、墓地、模板、用户 gfx/mods/sound 等不等于 save 目录；启动还会确保配置、存档、模板、用户字体/音效/图形目录存在。因此实例范围至少须涵盖完整 user 范围及明确配置、其他实际根，不能只比较 save。[用户资源路径](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/path_info.cpp#L294-L404)、[启动建目录](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L549-L569)。

调试日志为 config/debug.log，崩溃日志为 config/crash.log；日志初始化会追加写入并可能把旧文件改名为 `.prev`，不是只读检查。[日志路径](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/path_info.cpp#L266-L269)、[崩溃路径](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/path_info.cpp#L410-L413)、[调试日志写入](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/debug.cpp#L672-L705)。

JSON 加载器明确创建五类磁盘缓存：

| 逻辑根 | 磁盘缓存目录 |
|---|---|
| base | base/cache |
| config | config/cache |
| data | data/cache |
| memorial | memorial/cache |
| user | user/cache |

save 的解析缓存在此实现中传入空磁盘目录，注释和调用均表示不持久化该类 flatbuffer 缓存；这不等于存档本身没有其他写入。逻辑根分派与加载器调用决定具体缓存，而不是仅由 `PATH_INFO::cache_dir()` 的名称决定。[五类缓存及存档缓存](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/json_loader.cpp#L15-L68)、[根分派](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/json_loader.cpp#L71-L106)。

缓存初始化可能删除过时/多余文件，缓存生成会建目录并写 `.fb` 文件。因此共享 base/data 根属于潜在写入冲突，不能因为源 JSON 是只读资源便忽略。当前查阅的命令行处理表没有独立 `--cachedir` 参数，不能虚构该选项消除此冲突。[缓存清理](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/flexbuffer_cache.cpp#L220-L268)、[缓存写入](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/flexbuffer_cache.cpp#L320-L352)、[参数处理表](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L294-L525)。

以上是已发现范围，不是整个游戏/SDL/系统库所有文件写入的穷尽证明。安装范围内其他资源修改、第三方库缓存、链接逃逸、启动器/子进程接管仍需对应构建的实测和审计。

## 版本证据不能冒充二进制验证

该提交的发布工作流在构建中生成 VERSION.txt，字段为 build type、build number、commit sha、commit url；Makefile 将其和可执行文件、资源一起复制到分发目录。它是发布侧车格式的官方证据，但本地用户可修改/复制侧车，侧车不与本地程序摘要自动形成可信绑定。[侧车生成](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/.github/workflows/release.yml#L224-L232)、[分发组成](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/Makefile#L512)、[分发复制](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/Makefile#L1214-L1217)。

程序版本字符串来自编译时 VERSION（某些 Windows 条件分支回退到 0.H），Makefile/CMake 使用 Git 信息生成该宏；不能将侧车的任意 version 字段当成程序亲自报告。[getVersionString](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/version.cpp#L1-L20)、[Makefile 生成宏](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/Makefile#L975-L981)、[CMake 生成](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/version.cmake#L1-L14)。

`--version` 打印编译版本、tiles/sound 选项及当时 data/user 路径，并在两遍参数处理之前退出；即使将它与路径参数一起调用，输出也不能证明那些路径覆盖已生效。执行未知程序仍是执行代码，不能只为探测而自动运行。[版本输出](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L219-L244)、[提前退出顺序](https://github.com/CleverRaven/Cataclysm-DDA/blob/5d069f2eaccd125d22b53617ba73ce7fab157a9f/src/main.cpp#L527-L544)。

## 后续开发门

1. 只读安装检查分别显示二进制格式/架构、摘要及侧车声明，标明证据来源和未验证项；侧车缺失、损坏或声称其他提交时不猜版本。
2. 候选绑定计划可以解释上述参数与潜在范围，但必须保留 `unverified` 和并发不可用，不能因本记录存在就提升本地安装状态。
3. 对明确来源、摘要、构建选项和平台的授权样本验证完整路径、受控参数、真实写入、链接及子进程行为；将证据绑定到适配器修订和精确程序身份。
4. 若共享安装，先解决 base/data 缓存隔离或按真实安装写入范围互斥；同安装多实例登记不等于同安装多开安全。
5. 实例启动与任务冲突模型须在启动前重验目录身份和全部写入范围；重启后的未知会话保留相应范围保护。未通过上述门前，继续保持通用启动的保守互斥。
