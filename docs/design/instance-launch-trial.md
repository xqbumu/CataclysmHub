# 实例参数试运行：CDDA 0.H 源码方案

日期：2026-10-28。状态：受全局互斥保护的参数试运行已实现；真实 CDDA 绑定、版本识别、完整隔离和并发未验收。

## 为何分为试运行

[固定上游提交研究](../research/cdda-0h-directory-evidence.md)表明，单独指定 `--userdir` 无法覆盖 XDG 配置及安装缓存。0.H 在 base/data/config/memorial/user 范围都可能写入缓存，部分路径还会清理旧缓存。源码证据不能确认用户导入的任意程序采用同一行为，因此不得将选择方案视为已识别版本，也不更新实例 `binding_status`。

## 已实现行为

用户在游戏库显式选择 `cdda-0h-source-trial-v1` 方案后，为一个已登记实例生成预览。固定参数依次为：

```
--userdir <instance-root>/
--savedir <instance-root>/save/
--configdir <instance-root>/config/
```

参数分别作为数组元素传递，保留空格及 Unicode，不经过 shell，不接受额外参数或路径覆盖。安装工作目录沿用导入记录，预览明确提示它可能写入缓存。此阶段不复制安装，不迁移原存档，不创建目录；实际写入由用户明确确认后的程序产生。普通安装启动行为保留。

预览前验证实例关联安装摘要；生成和启动分别校验可执行文件摘要、目录规范路径和与其他已登记范围分离。预览持有实例目录与安装工作目录的物理身份句柄，启动前拒绝同路径换目录。实例目录完整枚举上限 20000 项、32 层，遇到链接、特殊文件、不可读取条目或超限则拒绝；不是仅检查 save/config 两个名称。

预览沿用 5 分钟、单次消费及最多 100 份缓存；取消不执行，不产生会话或审计。启动仍须 `confirmed=true`，与普通启动共用后端全局游戏/任务互斥，防止绕过 UI。启动前再次检查当前实例、安装、参数及目录身份，过期、程序变化或目录变化必须重新预览。最终检查与外部程序打开路径之间仍有竞态；句柄不是程序的文件系统沙箱，不构成隔离证明。

## 会话与持久化

schema 8 为 session 增加可空 `play_instance_id` 外键、`launch_profile_id`、`instance_data_root`。普通历史会话三项为空，实例会话保留当次关联、方案和目录快照。启动意图与审计一起提交后才执行程序；退出结果记录原会话，UI 按实例展示最近会话。仍只监测直接进程，退出状态为 `exited_unverified`，不生成检查点或稳定性结论。

旧库升级先保存校验过的 SQLite 副本，迁移事务失败保留原版本；重启将遗留启动中/运行中会话标为 `interrupted_unknown`，保留实例关联与目录，不重放启动。未知会话仍阻止全部启动和现有工作区任务，人工确认仍只说明已关闭，不确认数据稳定。

## IPC 与边界

`create_instance_launch_plan({ instanceId, profileId }) -> LaunchPlan`；只接受上述方案 ID，错误包括 `PROFILE_UNSUPPORTED`、`INSTANCE_NOT_FOUND`、`INSTANCE_PATH_CHANGED`、`INSTANCE_SCOPE_UNSAFE`、`INSTANCE_SCOPE_LIMIT`、`INSTANCE_SCOPE_OVERLAP` 及既有启动错误。随后通过既有 `launch_game({planId, confirmed:true})` 执行。三个实例上下文字段也加入 Rust 生成的 LaunchPlan/Session 类型。

这不是验证命令：不执行程序探测版本，不把 `--version` 的提前退出当作目录生效证明，不修改安装版本或实例绑定状态。用户选择方案可以对导入程序进行试运行，但程序是否遵循参数仍未知。实例数据暂未纳入工作区扫描/备份，界面须明确提示；文件回读与游戏加载兼容性也独立。

下一阶段优先取得可追溯真实构建样本，识别二进制/构建来源，实测完整写入与子进程行为，并处理安装 base/data 缓存共享；之后才开放经验证范围上的并行启动和实例备份。未知范围仍全局保护。
