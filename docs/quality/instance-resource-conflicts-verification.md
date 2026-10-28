# 实例物理目录冲突预检验证

日期：2026-10-28。平台：macOS Apple Silicon；设计见 [物理目录预检](../design/instance-resource-conflicts.md)。

## 自动检查

最终完整 `cargo test -p hub-core --locked`：130 项通过。`npm run check`：49 项前端测试及格式、类型、生产构建通过。Clippy workspace/all-targets、Rust fmt、macOS debug .app 打包通过。

新增五项核心比较测试覆盖同目录、双向嵌套、分离目录、读读与读写差异、Unix 符号链接双向别名、缺失/非目录/未知格式保护、请求自身重叠、全局未知保护及超限不能返回部分“无冲突”。新增两项实例集成测试验证检查无目录/审计副作用、活动占用变化后重新读取、缺失实例根单列问题及未知实例 ID 拒绝；所有报告保持两个验证/并发标志为 false。前端集成测试验证启动被阻止时仍可检查、无需选择方案或启动、关闭结果后可重新检查、旧冲突被新范围问题替换且不会执行游戏。

中间一次完整回归中，既有 `foundation::initialization_is_persistent_and_cannot_overwrite_existing_paths` 在 drop 后立即重开阶段返回 WORKSPACE_LOCKED。单独重跑该用例通过，随后完整 130 项重跑通过。原因未定位，没有修改锁语义、忽略断言或重试掩盖产品错误。原始失败及复测输出分别保留在本机 `/tmp/cataclysm-hub-conflicts-tests.log` 和 `/tmp/cataclysm-hub-conflicts-tests-rerun.log`，该偶发性仍需后续关注。

## 原生只读演练

通过 SQLite backup API 复制此前自行建立的合成参数接收器元数据，在副本增加 `conflicts-qa-unknown` 未知会话，关联既有合成安装与实例根。调试应用指向独立副本；没有执行真实游戏，没有解除这条占用，没有使用玩家数据。

- 游戏库普通启动和实例试运行保持禁用；“检查 隔离测试实例 资源冲突”仍可用，方案选择框保持未勾选。
- 检查对话框列出安装目录（含共享缓存）、实例数据目录及时间；三项冲突分别为安装目录重叠、实例目录重叠、游戏完整写入范围未知的全局保护，均标出占用者和对应路径。
- 明确提示检查不预留资源、绑定未验证、多版本并发尚未开放；结果没有启动或解锁按钮。
- 关闭检查结果后，未知占用及启动阻止仍在。关闭验收应用后，用独立只读连接确认会话仍为 interrupted_unknown，operation_log 总数与演练前完全相同，schema 仍为 8。
- 对长路径显示进行了分行与紧凑字号调整，保留完整路径，不以省略路径隐藏共享范围。

独立副本位置由 `/tmp/cataclysm-hub-conflicts-qa-path` 记录。应用已关闭。本轮未验证真实双开、执行期租约、完整写入隔离或实例备份，也未在 Windows/Linux 执行原生演练。
