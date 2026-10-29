# 实例验收的目录观测工具

Python 3.9+ 标准库工具，不执行游戏，也不授予绑定/并发验收状态。完整步骤与边界见 [真实游戏验收](../../docs/quality/real-game-binding-acceptance.md)。

```sh
python3 -m unittest discover -s scripts/acceptance -p 'test_*.py'
python3 scripts/acceptance/observe_scopes.py --help
```

snapshot 为所有声明范围记录根身份、相对目录集合和文件 SHA-256；diff 解释新增/修改/移除与不允许变化的范围。输出必须是范围外的新文件，旧证据不被覆盖。不要将观察结果写入实例或游戏目录。内部链接不重复遍历，外部/悬空链接或特殊文件阻断观察判定。

观察不是全系统跟踪、全局原子快照或游戏备份；临时写入后删除可能遗漏。始终结合原生操作、独立保存/重载、精确构建、进程及故障证据。`within_declared_allowance` 不表示绑定已验证；`binding_verified` 和 `concurrency_accepted` 恒为 false。

## 单文件配置的静止应用／回滚循环

`check_config_cycle.py` 读取三份已有 snapshot，不启动或修改游戏。检查目标文件应用摘要、原字节恢复、权限、完整范围和目标以外变化；不把整个实例列为允许任意变化的范围。目标只允许 options 或新世界默认模组文件；输出必须是所有观察范围之外的新文件。

```sh
python3 scripts/acceptance/check_config_cycle.py --help
```

退出码：0 为文件循环条件匹配；2 为观测阻断（仍输出报告）；1 为参数、输入或输出错误。`game_loading_verified`、`audio_verified`、`new_world_defaults_verified`、`binding_verified`、`concurrency_accepted` 均恒为 false。报告中的 `snapshot_canonical_sha256` 是规范化 JSON 内容摘要，不是原始输入文件字节摘要或真实性证明。

完整命令、人工操作与游戏可能改写配置的处理见[配置人工验收](../../docs/quality/content-config-manual-acceptance.md)。静止循环期间不要启动游戏；实际游戏读取与声音观察使用独立阶段和基线。
