# 实例验收的目录观测工具

Python 3.9+ 标准库工具，不执行游戏，也不授予绑定/并发验收状态。完整步骤与边界见 [真实游戏验收](../../docs/quality/real-game-binding-acceptance.md)。

```sh
python3 -m unittest discover -s scripts/acceptance -p 'test_*.py'
python3 scripts/acceptance/observe_scopes.py --help
```

snapshot 为所有声明范围记录根身份、相对目录集合和文件 SHA-256；diff 解释新增/修改/移除与不允许变化的范围。输出必须是范围外的新文件，旧证据不被覆盖。不要将观察结果写入实例或游戏目录。内部链接不重复遍历，外部/悬空链接或特殊文件阻断观察判定。

观察不是全系统跟踪、全局原子快照或游戏备份；临时写入后删除可能遗漏。始终结合原生操作、独立保存/重载、精确构建、进程及故障证据。`within_declared_allowance` 不表示绑定已验证；`binding_verified` 和 `concurrency_accepted` 恒为 false。
