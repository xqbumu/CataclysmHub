# 资源占用验证记录

日期：2026-10-28。平台：本机 macOS Apple Silicon。对应 [资源占用设计](../design/resource-ownership.md)。

## 自动验证

- `cargo test -p hub-core --locked`：123 项通过。
- `npm run check`：前端 48 项通过，Prettier、TypeScript 与生产前端构建通过。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo fmt --all --check`：通过。
- `npm run desktop -- build --debug --bundles app`：macOS debug 应用打包通过。

新增 Rust 用例覆盖五类所有者的全局/目录占用及读写用途、取消中保护与终态释放、重启任务恢复与未知会话保留、结束 worker 的覆盖不误删游戏占用、超过历史窗口的未知会话仍阻止准入、人工确认审计写入失败不释放保护。原有真实 worker 终态写入失败测试增加占用列表为空的断言。前端用例覆盖旧未知会话进入确认入口，以及无活动占用时仍保留绑定/兼容性和并发未验证的说明。

## 原生桌面合成验收

从此前自行建立的合成参数接收器工作区，通过 SQLite backup API 创建独立元数据副本，没有使用玩家存档，也没有启动真实游戏。只在副本增加一条 `resource-qa-unknown` 未知会话及 51 条较新的终止会话，使未知记录不出现在 Dashboard 最近 50 条会话历史中。调试应用通过独立 `CATACLYSM_HUB_DATA_DIR` 指向该副本。

1. 概览安装卡显示“工作区忙碌或待确认”且禁用启动。
2. 游戏库资源面板显示一项未知游戏占用、安装工作目录、“可能写入”及全局互斥说明；没有将已知目录视为完整写入边界。
3. 点击“检查并确认游戏关闭”进入既有确认对话框，明确提示检查游戏及子进程且不标记存档稳定。
4. 点击“返回检查”后占用保留，启动继续禁用。
5. 对自行注入、无对应进程的合成记录再次进入确认并明确提交后，资源面板显示无占用，安装卡恢复“启动预览”。未执行预览或启动游戏。
6. 应用退出后独立只读核对：会话状态 `resolved_unknown`；对应 `process.user_acknowledged` 审计恰好一条；`PRAGMA user_version=8`。验收应用已关闭。

本机临时副本位置由 `/tmp/cataclysm-hub-resource-qa-path` 记录，不进入版本库。无证据证明此轮真实双开、完整游戏写入隔离、实例备份或其他平台原生行为；相关验收门继续保持未完成。
