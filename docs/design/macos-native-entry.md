# macOS 原生本体入口试运行

日期：2026-10-28。新增可显式选择的 `cdda-0h-release-macos-native-trial-v1`、`cdda-0i-1-macos-native-trial-v1`。它们是实验参数方案，不识别本地版本，也不提升绑定或并发信任。

## 适用条件

所选程序已位于真实 `.app/Contents/MacOS/<本体>`，且文件头符合已支持的 Mach-O 魔数。魔数只作格式形状检查，不是完整二进制解析、代码签名、架构或版本认证；执行权限与 SHA 仍按既有导入/启动流程检查。脚本、非本体包装入口拒绝为 BUNDLE_NATIVE_ENTRY_REQUIRED。

同一 Contents 下须有规范、未重定向的 Resources 普通目录和既有 SDL2 系列框架布局；Resources 符号链接或包外重定向拒绝，框架仍不能解析至 Resources 外。其他布局拒绝为 BUNDLE_LAYOUT_UNSUPPORTED。非 macOS 平台拒绝该方案。

常见官方发行包的 MacOS 入口是 shell 启动脚本，本方案不自动把它改为原生本体，不自动生成入口、更改 Info.plist、复制程序或重新签名。因此不能宣称任意官方包默认即可使用该方案；需要已有合适的本体封装。原有 Resources v1/v2 方案保留。

## 启动与范围

安装记录继续保存实际所选程序与其默认父目录。只有显式选择原生方案时，LaunchPlan.executable 保持该 MacOS 本体，working_dir 改为同包 Resources；固定 userdir/savedir/configdir 参数与实例内 HOME/临时/XDG 覆盖沿用 v2。预览展示实际程序、cwd、环境和摘要，不接受任意自定义环境或额外参数覆盖。

通过同一个 tokio Command 直接启动并 wait/reap 实际本体，不运行 open、LaunchServices、shell 或辅助启动器，不通过这些入口的退出推断游戏退出。启动时复核程序摘要、实例根、工作目录及实际程序父目录的物理身份、框架与角色目录。MacOS 与 Resources 分别持有目录句柄；预览后即使替换 MacOS 并放回同字节程序，也拒绝启动。实际资源保护仍按完整 .app 与实例根派生，旧会话重启后仍未知且保守保护；不因实际 cwd 与安装默认目录不同而漏掉 Resources。

全局互斥保持，保存成功、原生窗口识别和文件完整性仍是不同证据。版本/平台/具体封装没有通过验收时继续未验证。
