# 资源实测反馈的复核

用户反馈：正式发行包菜单有短促音效，无背景音乐；没有找到hub_qa_npc_needs。随后用户正常退出，Hub记录89505743会话退出码0。

## 假设与检验

1. 实例资源路径未被游戏扫描：启动日志明确扫描P1的mods；CDDA 0.I-1 path_info.cpp定义user_moddir=userdir/mods、user_sound=userdir/sound，mod_manager同时扫描内置和用户mods。当前布局与此一致，不据反馈盲目搬目录。
2. 资源格式/ID未被游戏识别：另复制P1到probe-data，使用同一官方游戏本体、隔离user/save/config路径运行--check-mods hub_qa_npc_needs。实际输出Checking mod Hub QA NPC Needs [hub_qa_npc_needs]，退出0，说明该本体能发现并加载校验这个模组。原P1源和原世界不参与此探针写入。
3. 用户在菜单中检索了ID或受分类筛选影响：游戏显示名是Hub QA NPC Needs；仍需在新建世界模组选择页确认，不能凭CLI校验代替用户实际建档。
4. 音效来自内置Basic回退：首个测试包复用了相同音频，人的听感无法区分，原验收设计不能证明来源。旧包musicset为空，无背景音乐是预期，不能据此判定播放失败。

## 后续验证设计

新增独立ID hub_qa_twotone / 显示名Hub QA Two Tone，menu_move和menu_error播放440/660Hz两个短音（0.12幅度，淡入淡出，中间静音），不含背景音乐。通过原生安装和现有保护备份/单文件应用流程切换，再请用户确认可辨认双音与新世界加载。不会替换原QA包、原安装或既有世界。

源码依据：[path_info.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.I-1/src/path_info.cpp)、[mod_manager.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.I-1/src/mod_manager.cpp)、[options.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.I-1/src/options.cpp)。源码作路径解释，实际可识别性以本轮同一游戏本体CLI输出为证；世界加载和新音效仍待人工复验。

双音包已通过原生来源登记、部署和发现，最新保护副本68173c11完成且文件校验通过。应用前的原生备份选择器多次没有提交值，工具随后报告用户切换界面；没有将其推断为产品选择器缺陷。尝试AX点击、键盘选择和截图定位均未确认选中，已请求用户仅选择最新副本，再继续现有校验与确认流程。当前SOUNDPACKS仍是hub_qa_basic，不宣称双音包已启用或播放。

原Mount Andrew的108文件保持原SHA-256。代码未作路径变更或放宽配置保护。原生模组选择、新世界进地图及双音听感仍未验收。
