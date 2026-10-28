# 实例完整备份验证记录

日期：2026-10-28。平台：本机 macOS Apple Silicon。设计见 [实例备份](../design/instance-backup.md)。

## 自动验证

- 完整 Rust 140 项通过，前端 50 项通过。
- Clippy workspace/all-targets、Rust fmt、Prettier、TypeScript、前端生产构建和 macOS debug .app 打包通过。
- 新增实例用例覆盖只读计划、存档与配置完整纳入且不混入工作区/另一实例、幂等重试、归档实例关联、源离线后校验、独立新目录恢复、原数据不变、源/备份目录替换、未知会话保护、活动声明使用实际实例根、schema 8 升级副本及 schema 9 失败回滚。
- v2 清单负例覆盖未知版本、错误范围、来源缺失/不匹配/非法 UUID/摘要以及擅自提升绑定/边界状态；v1 仍能读取。非法持久实例记录在计划阶段拒绝，无临时文件/任务。
- 前端新增来源选择用例，证明实例预览不调用工作区计划、取消不创建、切回工作区后来源正确。

v2 JSON Schema 与示例为本切片新增契约，JSON 语法检查通过；本机未安装 jsonschema 引擎，未声称执行了 JSON Schema 引擎校验。实际清单语义和归档验证以 Rust reader 的正负例及下述真实生成归档为依据。

## 原生桌面与独立命令演练

全部数据来自此前自行构建的合成参数接收器夹具，复制到独立临时工作区；实例内仅 config/options.json、save/world.dat 两个普通文件，共 44 B、4 条目。工作区另设 workspace-only.dat 哨兵，不使用真实玩家数据、不执行游戏。

1. 来源下拉明确区分工作区与实例数据根；实例预览展示两文件/四条目及“不含安装/共享缓存、绑定未验证”的说明。
2. 第一次取消预览后，备份目录为空、backup_task 为零，schema 从 8 迁移至 9。
3. 合法身份夹具明确确认后创建成功；卡片显示 INSTANCE DATA ROOT、实例名称/数据根、绑定未验证、最近校验通过、尽力快照。
4. 在原生界面执行“重新校验”，任务完成且保留实例关联。
5. 关闭应用，将本测试副本的 metadata 目录临时重命名为 metadata-offline。独立 verify-backup 与 restore-backup 命令均退出 0；新容器 data/ 的两个文件逐项 SHA-256 与原实例完全相同，不含工作区哨兵。原实例摘要和工作区哨兵原字节均未改变，随后恢复测试元数据目录原名。

成功归档 ID：6dc47443-e94a-491f-a52d-45eeb361efd3；实例 ID：3c40aab5-f5ad-406d-8023-3d8f9a9e278f。本机临时目录由 `/tmp/cataclysm-hub-instance-backup-valid-qa-path` 记录，保留 expected.json、verification.json、restore-result.json、result-summary.json 与实际归档，不进入仓库。验收应用已关闭。

## 保留的夹具负例

首次沿用旧的手工 SQLite 夹具，实例/安装 ID 为字面量 instance/game，违反本轮归档要求的 UUID 身份；自校验返回 ARCHIVE_INVALID，未发布归档。清单明确暴露此违规输入，因此未按不相关的目录/压缩猜测继续调试。失败的 .partial、回执与清单保留在 `/tmp/cataclysm-hub-instance-backup-qa-path` 指向的独立目录，不删除或改成成功证据。

为避免晚到归档阶段才失败，产品增加 INSTANCE_CONTEXT_INVALID 计划前检查及回归负例；没有放宽 UUID/摘要约束。另一份新夹具采用与公开登记接口同格式的 UUID，并通过 foreign_key_check 后完成以上正例。该负例不代表正常登记接口会生成无效 ID，也没有修改用户数据库以“修复”来源。

## 未覆盖边界

未执行真实游戏数据备份、游戏加载兼容性或真实并发验收；未测试 Windows/Linux 原生执行。实例备份不含安装缓存和根外文件；独立恢复不自动重新绑定实例，也不覆盖现有实例。当前仍全局互斥。目录句柄身份检查约束管理器自身操作，不能限制任意第三方进程；实例保留/清理策略尚未开放。
