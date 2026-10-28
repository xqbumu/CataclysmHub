# 文件格式与兼容演进契约

版本：0.1.0 · 日期：2026-10-27 · 状态：待评审 · 责任角色：备份与档案负责人。依据：PRD §10.3、§13.1、§14.1。

## 契约文件

| 契约 | JSON Schema | 正例 |
|---|---|---|
| 完整备份内部清单 v1 | [backup-manifest.schema.json](backup-manifest.schema.json) | [backup-manifest.json](examples/backup-manifest.json) |
| 外部回执 v1 | [backup-receipt.schema.json](backup-receipt.schema.json) | [backup-receipt.json](examples/backup-receipt.json) |
| 恢复目录检查结果 v1 | [restore-inspection.schema.json](restore-inspection.schema.json) | [restore-inspection.json](examples/restore-inspection.json) |
| 恢复验证报告 v1 | [restore-verification.schema.json](restore-verification.schema.json) | [restore-verification.json](examples/restore-verification.json) |
| 便携环境档案 v1 | [environment-profile.schema.json](environment-profile.schema.json) | [environment-profile.json](examples/environment-profile.json) |

Schema 使用 JSON Schema 2020-12，均为设计草案；示例是合成数据，不代表真实 CDDA 存档或实际生成的归档。receipt 示例的摘要仅为格式占位，不应当作有效备份证据。

## 备份语义规则

schema_version 必须为 reader 支持的值。scope=world 时 world_id 非空；scope=save_root 时 root scope 标识仍必需。source_token 是不包含用户名和绝对路径的本地范围令牌。所有条目相对于 data/，目录 size="0"、sha256=null；文件 SHA-256 为小写 64 位十六进制。

清单的 file_count 仅统计普通文件，entry_count 包括目录，total_bytes 为普通文件原始大小之和。JSON 数字仅用于受限计数或版本；所有 64 位字节数使用十进制字符串。SHA-256 计算针对原始字节；manifest_hash 针对归档中 manifest 的原始 UTF-8 字节，不能 parse/re-serialize 后再算。

compression_version 记录实际编码库/格式实现版本。根目录备份必须携带 world_contexts，逐世界记录相对根路径、最后游戏版本和档案/适配器信息；无法识别时上下文可为空，但未知范围须保留在 entries。顶层 game 在无法描述一个共同环境时为 null，不能用某个世界的版本代表全部世界。world_contexts 的相对路径和身份唯一性须作语义校验。

Schema 验证不能代替语义验证。运行时还必须检查路径规范、安全类型、唯一性、祖先冲突、目标平台碰撞、总数/大小、归档中无多余/缺失文件、scope/world 关系和 reader 资源限额。文件清单已知类型必须完整保存；未知字段在 v1 备份核心对象中拒绝，未来兼容扩展通过 extensions 对象携带并保持不透明。

receipt 中 verification=verified 只是历史记录；恢复仍重新验证当前归档和载荷。无可信签名时摘要不提供来源认证。发布顺序与回执自引用规则见 [备份设计](../design/backup-and-restore.md)。

## 环境档案语义规则

profile_id 是导出档案身份；game 用分支、版本和平台描述，不使用本机 installation_id 作为跨机唯一定位。content 保留资源类型、角色、修订、启用状态、顺序、来源和可选摘要。launch_args 为字符串数组，导出前筛除绝对个人路径、秘密和不便携参数，并在 omitted_fields 说明省略类别。

config_refs 仅保存可移植相对路径和摘要。未知字段允许在同主版本中读取和原样保留，但不执行其行为；未知资源类型不能启用。未知主版本拒绝应用，可查看原文和导入报告。导入先给出 missing/conflict/platform_difference/unknown_fields，再按确认计划写入。

## 演进策略

添加纯描述扩展可用兼容扩展区；修改路径、摘要或恢复语义必须新 schema_version，并提供旧格式读取与迁移测试。不得就地覆写旧备份来升级格式；迁移生成新归档，验证通过后由用户决定旧归档保留。应用版本、数据库版本、清单版本、档案版本彼此独立。


## 新目录恢复报告

restore-verification.json 由已实现的独立/桌面恢复核心生成。示例来自合成 fixture 的原生恢复演练，不含私有绝对路径。schema_version=1、state=verified 只描述写入时逐项回读结果。restored_files / restored_entries / restored_bytes 必须与 backup 摘要里的对应值一致；仍须通过运行时语义校验，不能仅凭 Schema 或报告自述判定任意目录当前健康。

报告不携带自动切换存档指令，不认证归档来源，不把 best_effort 升级为游戏语义一致。未完成标记存在、命令退出失败或任务为 recovery_required 时，不能按报告单独宣称恢复流程成功。桌面数据库与目录报告的提交不是跨介质原子事务。


## 恢复目录只读检查结果

verify-restored 输出 restore-inspection v1，嵌套复用 restore-verification v1 的字节验证摘要，不写回任何报告文件。inspected_at 是本次目标检查结束时间；嵌套 backup.verified_at 是本次源归档验证时间。incomplete_marker_present 与 historical_report_present 仅说明对应普通标记文件存在，不认证其 JSON 内容、摘要或原流程状态。

检查 data/ 的精确载荷集合；容器外层的笔记等额外文件不属于载荷集合，历史报告不能代替本次核对。错误码 RESTORED_DATA_MISMATCH 表示载荷缺失、不完整可读、类型/集合/大小/摘要不匹配；PERMISSION_DENIED 保留明确的权限原因；RESTORE_MARKER_INVALID 表示控制标记不是普通文件（包括链接），不能按正常容器解释。源归档错误沿用原归档校验错误码。

Schema 以相对 $ref 引用 restore-verification.schema.json，应从同目录或本地资源注册表解析，不需要网络。对应示例来自合成原生恢复产物的只读检查，不含私有路径。标记观察和载荷检查不是原子快照，操作时应关闭外部写入者。

## 当前实现补充：实例清单 v2

实例数据根备份使用 [backup-manifest-v2.schema.json](backup-manifest-v2.schema.json)，[示例](examples/backup-manifest-v2.json)为合成空范围，不是有效归档证据。v2 明确 scope=instance_data_root、source_token=实例 UUID，extensions.instance_source 保存登记安装 UUID/SHA-256 和未验证状态；不写源绝对路径。读取器还要求 source_token 与实例 UUID 一致、所有路径/摘要/计数/预算校验通过，不接受擅自提升绑定/边界状态。当前 v2 仅 best_effort，game/profile/adapter/world/character 等未实现语义不作兼容声明。

工作区清单仍为 v1/save_root，旧 v1 的读取与新目录恢复保留；外部 receipt 仍为 v1，仅描述归档/清单摘要，不代表内部清单也必须为 v1。旧 reader 遇到 v2 应明确拒绝。恢复的 data/ 包含实例根下的相对路径，不自动复原安装或登记游玩绑定。

## 当前实现补充：独立安装副本控制文件 v1

新容器包含 payload/<源目录名>/、copy-manifest.json、copy-verification.json；未完成阶段保留 INSTALLATION_COPY_INCOMPLETE.json。此格式不是备份归档格式，不被 backup reader 自动识别或登记。

清单 v1 记录复制 UUID、创建时间、payload 根、普通可执行文件相对路径与 SHA-256、根权限及完整条目。条目记录相对路径、directory/file/symlink 类型、十进制大小、可空摘要/相对链接文本、链接目标目录提示、可空 Unix 模式及 readonly。原生平台权限语义不保证跨平台等价，不写本机源绝对路径。

回执 v1 绑定清单摘要、复制 ID、payload/程序路径与摘要、文件/链接数、字节量和观察时间；binding_verified/isolation_verified 必须为 false。serde 拒绝未知控制字段，读取受大小/条目/路径约束，并全量重读当前 payload；不能通过修改回执提升来源、绑定或隔离证据。未完成、内容/集合变化、额外控制或同级 payload 均拒绝。清单/回执未签名，不提供官方来源认证。详情见 [复制与验证协议](../design/installation-copy.md)。
