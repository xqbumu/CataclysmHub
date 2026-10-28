# 真实游戏绑定与多版本并发验收

日期：2026-10-28。目标：USR-001。应分别判断游戏行为实验与 Hub 产品能力，不能用进程存在、菜单截图或参数接收器替代真实存档验收。

## 验收顺序

1. **固定样本身份。** 使用明确来源的完整发行包，分别记录下载 URL/资产 ID、包摘要、游戏本体摘要、二进制架构、程序自报构建、上游源码提交及资源组合。发布标签、用户名称、VERSION.txt 与本体身份分别记录；不执行任意未知安装来猜版本。
2. **建立测试环境。** 两个版本分别使用完整安装副本和全新的玩家数据目录；测试 HOME/配置/缓存/临时目录也独立。不得直接载入唯一原存档。对安装、实例和测试保护目录做基线采集；证据输出存到这些范围外。
3. **逐实例真实游玩。** 在游戏界面创建新世界和角色，推进操作、保存、退出世界、重新加载，确认加载的是同一实例的数据。检查配置、日志、缓存、纪念记录及其他写入，不能只看 save。记录程序是否直接运行、启动器是否退出和是否有子进程接管。
4. **双实例交叉验证。** A/B 同时进入各自世界；A 保存和重载时 B 保持运行，比较 B 的范围；再反向执行。退出 A 后 B 仍能操作和保存。两个菜单或两个 --check-mods 进程不算双实例游玩。
5. **Hub 集成验收。** 由 Hub 启动并监测两个实例，按真实安装及玩家数据范围持有资源租约；退出 A 不释放 B 的保护。备份/恢复只与涉及范围冲突；未知启动器或未验证安装仍保守阻止。共享、嵌套、目录别名、参数覆盖都须有负例。
6. **故障与发布验收。** 管理器重启/强杀、游戏异常退出、PID 复用、子进程接管、目录替换、断盘、审计失败均不能误放行。重启未知实例保留原范围保护；来源/摘要/适配器修订变化使旧证据失效。平台、具体构建、资源组合与样本范围逐项声明，不泛化为所有 CDDA。

游戏可加载、文件完整性、完整目录绑定、并发调度和退出后存档稳定性是不同结论。

## 当前 Hub 操作

游戏库导入本体，在 macOS SDL2 发行包中选择 `Contents/Resources/cataclysm-tiles`，不要选不转发参数的 bundle shell 脚本。为安装登记现有独立空目录。在“试运行参数方案”选择相应方案，再显式勾选确认：

- `cdda-0h-source-trial-v1`：原有通用源码参数，不覆盖进程环境。
- `cdda-0h-release-macos-trial-v1`：H 发布包布局试运行。
- `cdda-0i-1-macos-trial-v1`：I-1 发布包布局试运行。

macOS 方案检查 Resources/Contents 布局及包内 SDL2 系列框架目标，再给**当前游戏进程**设置 DYLD_FRAMEWORK_PATH/DYLD_LIBRARY_PATH=该 Resources，不改变系统环境；实际值显示在预览。用户选择方案仍不证明本地程序属于该版本。

预览只读，确认后先持久记录启动意图，再通过已核对身份的目录句柄准备 save/config 子目录，然后执行固定目录参数。已有普通目录可复用；文件/链接占位拒绝，绝不覆盖。准备或执行失败保留明确会话结果，可能留下新建空目录；不为清理而删除用户文件。实际绑定状态仍为未验证。

**当前 Hub 仍全局互斥，不能通过这一界面完成第 5 步的双开验收。** 下节实验由独立测试启动器完成，不以临时关闭生产锁来冒充产品支持。

## 本机已执行证据

[正式实验记录](real-game-macos-2026-10-28.md)和[摘要/本地证据索引](evidence/real-game-macos-2026-10-28.json)记录官方 H、I-1 本体在独立安装/玩家目录中的真实创建、保存、退出、重载及两个版本同时进入世界。H 操作阶段没有观测到 I 的范围变化，反向同样如此；H 进程退出后 I 仍响应。

这些已执行用例缩小了绑定不确定性，但并未穷尽所有 OS 写入、资源组合、故障及平台，也未由 Hub 并发启动/调度，因此不提升绑定标记，不宣称 USR-001 完成。

## 可重复的文件观测

项目提供不执行游戏的标准库工具：

```sh
python3 scripts/acceptance/observe_scopes.py snapshot \
  --scope install-a=/test/install-a --scope install-b=/test/install-b \
  --scope instance-a=/test/instance-a --scope instance-b=/test/instance-b \
  --scope protected=/test/protected --output /test/evidence/before.json
# 在真实游戏中完成单个阶段，等待保存结束
python3 scripts/acceptance/observe_scopes.py snapshot \
  --scope install-a=/test/install-a --scope install-b=/test/install-b \
  --scope instance-a=/test/instance-a --scope instance-b=/test/instance-b \
  --scope protected=/test/protected --output /test/evidence/after.json
python3 scripts/acceptance/observe_scopes.py diff \
  --before /test/evidence/before.json --after /test/evidence/after.json \
  --allow-change install-a --allow-change instance-a \
  --output /test/evidence/diff.json
```

snapshot/diff 仅写新的证据文件，不修改所选范围、不覆盖已有输出。返回 0 只代表采集完成或变化在声明允许范围；返回 2 表示观测有阻断项，返回 1 表示无法完成。名字和物理目录重复/嵌套、输出落入观察范围、根身份变化、范围集不一致、外部/悬空链接、特殊文件、无效/过大文档、预算超限均不能静默通过。内部框架链接只记录、不沿链接再次读取。限制 100000 条、64 层、8 GiB 文件逻辑读取、64 MiB 证据文档。

这不是 OS 写入跟踪、进程监测或沙箱；运行中的目录采集也不具备全局原子性。两个采集点之间产生后又删除的临时文件可能遗漏，观察范围之外的写入未覆盖。产物始终 `binding_verified=false`、`concurrency_accepted=false`；需要结合受控环境、实际游玩、进程证据和完整验收矩阵。

## 新增 v2 运行环境试运行方案

`cdda-0h-release-macos-trial-v2` 与 `cdda-0i-1-macos-trial-v2` 在原 v1 的固定参数和包内框架路径基础上，将 HOME、TMPDIR/TMP/TEMP、XDG_CONFIG_HOME/XDG_CACHE_HOME/XDG_DATA_HOME/XDG_STATE_HOME 分别定向至实例 `.hub-runtime/{home,tmp,config,cache,data,state}`。桌面须明确选择方案并重新勾选试运行确认，预览展示全部实际覆盖值。

预览只检查不创建；确认后先记录启动意图，再通过实例能力句柄逐层创建/打开普通目录，新建 Unix 环境目录为 0700。不覆盖文件、不沿链接准备、不修改已有目录权限、不自动清理临时文件。旧 v1 不获得这些覆盖；普通安装启动仍使用原环境。

其他环境仍继承，macOS 系统 API、绝对配置路径或游戏自身规则可能忽略这些变量；不宣称全部写入都受控。`.hub-runtime` 位于实例整根备份范围，临时文件会占用容量，链接或特殊文件仍受备份既有拒绝规则约束。不能因此解除共享安装缓存冲突或全局互斥。

合成子进程已验证实际环境和写入目标；H/I 官方游戏尚未在该 v2 Hub 路径下完成游玩、保存和重载验收。下一轮应使用独立安装副本，为各版本登记新实例，采集声明范围基线，然后由 Hub 顺序启动进行真实存档验收。

## Hub v2 顺序启动实验补充

[本轮实际记录](hub-real-game-v2-2026-10-28.md)已确认 H/I 本体由 Hub IPC 顺序启动、实例配置/日志写入及退出监测。原生游戏窗口定位和 I 温缓存旧数据错误阻断了真实世界保存与重载，故本轮不计游玩或绑定通过。实际启动后观察到的安装写入再次说明 Resources cwd 不能当作完整隔离保证。已知 .app 的保护范围现扩大为整个包，旧会话重启后同样适用；普通目录安装保持工作目录范围。

## 原生本体入口与 Hub I 新证据

新增 cdda-0h-release-macos-native-trial-v1 / cdda-0i-1-macos-native-trial-v1，适用于已经有 Contents/MacOS 下原生 Mach-O 本体的包。导入实际本体，方案以同包 Resources 为 cwd，仍直接监测游戏 PID；不会自动生成入口、更改 Info.plist 或运行官方包装脚本。原始默认脚本包不能因此宣称已适配。

[本轮 Hub I 验证](macos-native-entry-verification.md)完成固定本体的新世界/角色、保存、同进程卸载重载、继续操作、再保存和正常退出。数据均落于新实例的声明范围，两个对照实例和保护哨兵在采集点未变化。绑定标记不自动提升；H、跨进程重载、完整 OS 写入和 Hub 并发仍待验收。
