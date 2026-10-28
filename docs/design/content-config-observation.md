# 实例模组与声音配置引用观察

日期：2026-10-28。状态：只读子集实现，未开放配置写入或游戏资源发现。

## 实际读取范围

从已登记实例的 data_root 读取 Hub 参数布局中的 `config/options.json`、`config/user-default-mods.json` 和 `save/<目录候选>/mods.json`。不递归读取角色、地图或音频内容，不执行游戏，不新建目录，不持久写入报告，不改变绑定、任务占用或未知会话状态。未知安装版本也可按此布局观察文件，但格式可解析不等于版本匹配或绑定验证。

- options 按 name/value 条目数组读取 `SOUNDPACKS` 和 `SOUND_ENABLED` 原字符串；重复相关设置或字段类型不支持时不选择赢家，不推断默认包或布尔值。
- 新世界默认模组读取 `MOD_INFO` 对象、`id=user:default` 的 dependencies，不套用已有世界的 ID 数组格式。
- 世界目录候选的 mods.json 按字符串数组读取，保留原声明顺序与重复项，不应用上游的废弃 ID 替换或去重规则。目录候选并非经过完整游戏世界身份验证。
- 按 ID 汇总引用文件并匹配资源库登记快照；保留多个匹配以显示歧义。没有登记匹配不等于游戏缺失资源，尤其不能把内置 `dda` 判为缺失。

返回读取时间、文件 SHA-256、逐文件状态、引用路径与候选匹配。缺失、不可读、非法路径、损坏 JSON、格式不支持和超限均显式区分，只有确实读取到 `[]` 才显示“显式空列表”。save 根缺失或不可访问单独报告范围问题。

## 范围与限制

使用能力目录句柄逐级 nofollow 打开配置和世界路径，拒绝链接／非普通文件；单文件最多 1 MiB、总读取预算 16 MiB，save 根最多 512 项／128 个目录候选，单模组列表最多 256 项。目录枚举超过限制整次报错，不返回“完整空结果”。结束时复核实例根的物理身份。

允许游戏运行期间只读观察；Hub 的 mutation 门不能阻止游戏或其他程序改文件，因此多个文件不是同一时刻快照，文件 SHA-256 也不是未来写入授权。报告始终 binding_verified=false，不新增数据库表或租约。

本轮尚未扫描安装内置或用户资源目录，未判断资源是否被实际发现或加载；“资源库匹配”“配置引用”和“游戏加载成功”必须分开呈现。引用表是升级／移除前的人工影响核对线索，不是完整引用审计或可执行配置变更计划。

## 依据

- [CDDA 0.H path_info.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.H/src/path_info.cpp)：用户配置、默认模组与存档目录。
- [CDDA 0.H mod_manager.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.H/src/mod_manager.cpp)：默认组合对象、世界 mods.json 顺序列表，以及加载时可能执行的 ID 替换。
- [CDDA 0.H options.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.H/src/options.cpp)、[0.I-1 options.cpp](https://github.com/CleverRaven/Cataclysm-DDA/blob/0.I-1/src/options.cpp)：配置序列化与字符串值。这里只实现可观察字段，不认证全部版本语义。
- 本项目 `instance_launch::profile_args` 显式传递 userdir、savedir、configdir；观察该计划布局不证明游戏实际采纳。

## 后续

先结合明确的游戏安装／版本扫描实际资源位置与元数据，检查依赖和重复 ID；然后生成实验世界模组组合、声音配置的差异与保护建议。已有世界的模组增删、跨版本迁移、升级／卸载仍需单独存档影响与引用检查，不根据此报告直接执行。
