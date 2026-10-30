# 游戏语言修改原生验收

最终状态：简体中文主菜单已由用户确认，正常退出及存档完整性核验通过。

2026-10-30。范围为已识别macOS CDDA游戏样本的独立P1 QA实例，不修改日常实例或Hub界面语言。

修改前USE_LANG为空，即跟随系统，不能等同指定中文。游戏安装存在lang/mo/zh_CN/LC_MESSAGES/cataclysm-dda.mo。

通过Hub原生配置界面选择简体中文，生成预览并创建完整保护副本5188d09a；校验该副本后明确确认应用config/options.json，记录84722834状态applied。解析前后仅USE_LANG值从空变为zh_CN，其他选项包括声音包不变；启动前193个存档文件保持原SHA-256。

通过Hub受控启动会话640558a6。游戏日志两次记录Setting language to: zh_CN，C/C++ locale均为zh_CN.UTF-8。配置写入及游戏读取已核实；实际主菜单简体中文及正常退出待用户确认，不将日志选择语言等同所有文本完整翻译。

[结构化记录](evidence/game-language-native-2026-10-30/application-and-launch.json)、[本轮启动日志](evidence/game-language-native-2026-10-30/startup-log.txt)。本轮没有代码修改或重跑自动测试，不扩展其他语言、游戏版本及平台认证。

## 最终确认

用户回复“菜单已中文并退出”。会话640558a6已结束，退出码0，进程不存在。退出后193个存档文件与修改前清单及SHA-256完全一致；解析后的选项差异仍只有USE_LANG从空变为zh_CN。该实例的语言修改已完成“Hub写入 → 游戏读取 → 实际菜单中文 → 正常退出”的闭环，未发现需要修复的代码问题。验证不代表所有游戏文本均有翻译。

[最终结果](evidence/game-language-native-2026-10-30/final-result.json)。
