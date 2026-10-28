# 工作区锁重开失败诊断与修复

日期：2026-10-28。平台：本机 macOS Apple Silicon。范围：正常释放最后一个 Hub 后，立即重开偶发 WORKSPACE_LOCKED；不是游戏写入范围锁或存档兼容性问题。

## 复现与证据

原问题记录在 [实例资源预检验证](instance-resource-conflicts-verification.md)。未先更换锁库、加入延时或忽略失败，而是构造可执行反馈循环：编译 foundation 集成测试，直接运行相同测试二进制，每轮 `--test-threads=16`。

- 修复前 60 轮在第 48 轮复现一次相同重开边界的 WORKSPACE_LOCKED，发生于 `restart_does_not_misreport_an_unfinished_process_as_exited`。原最初失败位于初始化重开用例；两个用例均为释放 Hub 后立即打开同一元数据目录。
- 临时定点 instrumentation 后再次复现：第 15 轮失败，释放前 `Arc::strong_count=1`，锁 API 返回 OS error 35、WouldBlock。排除该次由额外 Hub 引用未释放或其他 OS 错误被误分类导致的解释。
- 最小确定性回归通过 `File::try_clone` 保留同一锁的描述符引用，再 drop 唯一 Hub，立即重新 `Hub::open`。修复前稳定失败，修复后通过。它模拟 dup/fork 共享内核锁引用的实际释放缺口，不靠 sleep 调时序。

一手语义依据：Apple 明确说明 dup/fork 是同一 flock 的多个引用，显式解锁会影响同一锁；Linux man-pages 说明仅在全部重复描述符关闭或显式 LOCK_UN 后释放。fs2 的 Unix 实现直接使用 flock。来源：[Apple flock 手册](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/flock.2.html)、[Linux flock 手册](https://man7.org/linux/man-pages/man2/flock.2.html)、[fs2 锁语义](https://docs.rs/fs2/0.4.3/fs2/trait.FileExt.html#notes-on-file-locks)。

## 假设与定位范围

事先列出并检验三个假设：进程创建期间描述符暂时继承、异步任务仍持有 Hub、非占用错误被统一显示。本次原始失败的 strong_count 与 errno 支持第一个方向，确定性描述符复制用例证明“仅关闭一个句柄”确有生命周期缺口。

尚未对原始竞争实施系统调用跟踪来锁定具体子进程和 fork/exec 时点。因此可确认并修复共享描述符下的释放缺口，不能声称已追踪原始失败的每一次内核事件；也不据此推断其他文件系统、平台的全部锁异常均已解决。

## 最小修复

使用私有、不可 Clone 的 WorkspaceLock guard 管理已成功取得的锁。最后一个 Hub 释放时，guard 先调用与加锁库配对的显式 unlock，再关闭描述符。只有加锁成功后才构造 guard，失败的竞争者不会解锁当前所有者。Hub 初始化后续失败也通过同一 RAII 路径释放锁。

没有迁移 fs2、改变元数据 schema、加入重试/延时、删除锁文件或放宽单写者约束。解锁失败仅记录不含路径的 WORKSPACE_UNLOCK_FAILED，并继续关闭句柄；不会冒充释放成功或强行删除锁文件。guard 位于 Hub 最后字段，实际工作者保有 Arc 时 guard 不会提前释放。

回归同时验证：活着的 Hub 始终拒绝第二个打开者；最后 Hub drop 后即使重复描述符仍在也可重开；旧重复描述符随后关闭不能影响新所有者的锁；另一个 Arc 所有者仍在时不能重开，只有最后所有者结束才释放。

## 验证与边界

修复后使用与修复前相同的 foundation 并行测试循环执行 120 轮（1320 项用例执行，约 89.09 秒），零失败。完整 Rust 132 项、前端 49 项、Clippy、fmt、TypeScript、前端构建通过；构建记录见 [实现状态](../delivery/implementation-status.md)。原始/仪器化/最小回归修复前失败的日志保留在本机 `/tmp/cataclysm-hub-lock-stress-failure-47.log`、`/tmp/cataclysm-hub-lock-instrumented-failure.log`、`/tmp/cataclysm-hub-lock-duplicate-before.log`；修复后压力摘要为 `/tmp/cataclysm-hub-lock-stress-after.json`。原始日志含临时诊断标签，代码中已清除全部标签。

此修复面向正常析构；SIGKILL、系统崩溃不会运行 Drop。子进程/文件系统的锁释放仍服从操作系统语义。会话 interrupted_unknown 的全局保护与人工确认完全保留，不能把解开 Hub 元数据锁解释为游戏已经退出。Windows/Linux 本轮未实际执行该压力实验，Unix 重复描述符负例也不替代平台验收。

预防措施是把锁的取得/释放封装为单一所有权对象，并保留描述符复制与多 Arc 生命周期负例，而不是默认换一个开源锁库就会改变内核行为。
