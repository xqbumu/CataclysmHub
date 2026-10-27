---
status: proposed
date: 2026-10-27
---
# 内置清单与外置验证回执

PRD §10.3 同时要求清单、归档摘要与验证结果。建议 TAR/Zstandard 归档内部包含 manifest 与 data，归档外 receipt 保存归档哈希和验证结果，避免把归档自己的摘要放进其内容形成自引用。

## Considered Options

全部元数据放外部会使单一归档丢失恢复解释；全部放内部无法自然记录最终归档摘要。双层封装允许归档单独恢复，receipt 提供完整封装校验和本地验证记录。

## Consequences

目录搬移需包含两个文件；receipt 丢失时可重新验证内部载荷并重建本地回执，但不能因此证明来源可信。schema 与独立 reader 一起演进。
