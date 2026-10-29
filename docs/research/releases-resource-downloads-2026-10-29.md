# 游戏发布与模组、声音下载来源调研

调研日期：2026-10-29。目标是为官方版本列表、手动刷新、资源下载管理提供一手证据；本文的候选和建议不等同已实现或已通过游戏兼容验收。

## 开源启动器采用的模式

Catapult 在固定清单中维护游戏仓库、资产过滤、模组包和声音包；游戏使用 GitHub Releases，声音同时采用 release asset 与分支 ZIP，模组使用分支 ZIP 和明确包内子目录。它支持下载后整体或选择性安装，不宜把包中所有模组默认启用。其清单仍包含旧维护者及互联网归档来源，因此不能照搬为“全部当前推荐”。本次固定参考 Catapult commit `f90f1cf89ee285f1624baab47e5cc916b727a364`。[发布来源](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/data/release_sources.json)、[模组清单](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/data/modpacks.json)、[声音清单](https://github.com/qrrk/Catapult/blob/f90f1cf89ee285f1624baab47e5cc916b727a364/data/soundpacks.json)。

## 可直接接入的官方游戏仓库

对每个仓库调用 `GET https://api.github.com/repos/{owner}/{repo}/releases`；下载只取返回的 assets，不能把自动生成的源代码 ZIP 当作可玩版本。

| 变体 | 官方仓库 | 本次实查与资产特点 |
|---|---|---|
| Dark Days Ahead / DDA | [CleverRaven/Cataclysm-DDA](https://github.com/CleverRaven/Cataclysm-DDA/releases) | 发布活跃；实验版标记 prerelease；Linux tar.gz、Windows ZIP、macOS universal DMG，另有 Android 与翻译资产 |
| Bright Nights / BN | [cataclysmbn/Cataclysm-BN](https://github.com/cataclysmbn/Cataclysm-BN/releases) | 发布活跃；不要继续使用旧组织 cataclysmbnteam；macOS 区分 ARM / x64，部分资产带或不带声音 |
| The Last Generation / TLG | [Cataclysm-TLG/Cataclysm-TLG](https://github.com/Cataclysm-TLG/Cataclysm-TLG/releases) | 本次返回 2026-10-29 发布；仓库标记 prerelease=false，并不构成本应用稳定性认证 |
| Era Of Decay / EOD | [AtomicFox556/Cataclysm-EOD](https://github.com/AtomicFox556/Cataclysm-EOD/releases) | 端点仍可用；本次最新为 2024-04-11，展示日期，勿称持续更新 |
| There Is Still Hope / TISH | [Cataclysm-TISH-team/Cataclysm-TISH](https://github.com/Cataclysm-TISH-team/Cataclysm-TISH/releases) | 端点仍可用；本次最新为 2023-10-25，展示日期，勿称持续更新 |

各变体发布来源同时由 Catapult 的上述清单及仓库自身证实。EOD 与 TLG 的 README 明确把下载指向自身 Releases。[EOD README](https://github.com/AtomicFox556/Cataclysm-EOD#downloads)、[TLG README](https://github.com/Cataclysm-TLG/Cataclysm-TLG#downloads)。BN 项目声明不维护或背书第三方启动器；Hub 不应表示获得 BN 官方支持。[BN README](https://github.com/cataclysmbn/Cataclysm-BN)。

建议保留原始文件名、系统/架构提示、发布时间、上游预发布标记与发布说明链接。macOS DMG 需要独立安装流程；下载成功不能直接记作已安装。不得用模糊 x64 字符串把 ARM 或 universal 资产误排除，也不能只展示每个发布的第一个资产。

## 资源源头与下载模式

| 来源 | 下载模式 | 目标/维护信息 | 建议 |
|---|---|---|---|
| [Fris0uman/CDDA-Soundpacks](https://github.com/Fris0uman/CDDA-Soundpacks) | GitHub release assets：`CC-Sounds.zip`、`CC-Sounds-sfx-only.zip`、`CO.AG-music-only.zip` | 作者 README 区分完整包、仅音效、仅音乐；下载后选择声音包并重启 | 首批声音源；按 release asset ID 和 digest 保存来源 |
| [Kenan2000/CDDA-Structured-Kenan-Modpack](https://github.com/Kenan2000/CDDA-Structured-Kenan-Modpack) | master 分支 ZIP，另有历史 release `Modpack_Test_Version` 的 `CDDA-Kenan-Modpack_Test_Version.zip`（本次 18,310,176 字节、无上游 digest）；旧名称 `CDDA-Kenan-Modpack` 会重定向 | **README 明确停止更新**；主要模组位于 `Kenan-Structured-Modpack/High-Maintenance-Huge-Mods` | 可作历史资源来源，但标记停更，不标成适配最新 DDA；不自动全选 |
| [Zlorthishen/BrightNights-Structured-Kenan-Modpack](https://github.com/Zlorthishen/BrightNights-Structured-Kenan-Modpack) | master 分支 ZIP，本次无 releases | BN 专属；High-Maintenance-Huge-Mods 与 Medium-Maintenance-Small-Mods 两层分类 | 首批 BN 模组源；先解析 commit SHA，再下载固定 commit ZIP |
| [NarandBD/Otopack-BN-Mk-2](https://github.com/NarandBD/Otopack-BN-Mk-2) | main 分支 ZIP，声音位于 `Otopack+ModsUpdates BN` | README 说明已随 BN nightly 捆绑；不能把同名重复包当成必须下载 | 可选 BN 声音源，先检查已有安装；不要以空资产 release 当下载包 |

CC-Sounds 项目有顶层 CC-BY-SA-4.0 许可证，README 说明具体音效归属记录在各目录 credits.md，音乐单独有授权文件；下载和提取时应保留这些文件，不把整个音频集合统一宣称 CC0。[README 与许可证](https://github.com/Fris0uman/CDDA-Soundpacks)、[音乐授权](https://github.com/Fris0uman/CDDA-Soundpacks/blob/main/CO.AG_authorisation.txt)。模组合集与 Otopack 的 API 顶层 license 字段为空，并不证明每个文件无许可；应保留上游来源和逐包说明，不给出统一再分发许可结论。

Arcana 仓库当前已归档，作者说明 BN 版本已经入主仓库、DDA 版本不再维护，不宜作为新的“活跃推荐下载”。[作者 README](https://github.com/chaosvolt/cdda-arcana-mod)。

补查 [Cataclysm++ / Noctifer-de-Mortem/nocts_cata_mod](https://github.com/Noctifer-de-Mortem/nocts_cata_mod/releases) 仍有提交活动，并有 `BN-0.12`、`BN-0.11.1` 发布，但这些发布没有附件，应使用明确 tag/commit 源码 ZIP，不应显示不存在的下载资产。[Goats-Mod-Compilation](https://github.com/TheGoatGod/Goats-Mod-Compilation/releases) 虽有历史 ZIP，发布停留在 2021，不能作为活跃模组源的替代证据。

## 同步、缓存与下载实现建议

GitHub Releases API 只列已关联 release 的标签，公开仓库可匿名读取；分页默认 30、最大 100。资产含 name、size、browser_download_url，新的资产可有 `digest`，旧资产可能为空。读取 release list 与 latest 可分别覆盖近期实验版和稳定入口；若有截断必须显示“近期列表”，不能称完整历史。[Releases API](https://docs.github.com/en/rest/releases/releases)。

下载使用 API 返回的 browser_download_url，或 asset API 的 octet-stream 响应；后者可能直接返回内容或 302。对已固定来源、资产 ID、预期大小和可用 SHA256 做核验；无上游 digest 时记录本地 SHA256，但不能宣称完成上游真实性校验。[Release assets API](https://docs.github.com/en/rest/releases/assets)。

用户要求的“首次自动同步，以后手动更新”适合落盘缓存：按 source 记录首次尝试、成功时间和错误；重启、切页、组件重挂载不暗中刷新。首次失败提供明确的手动重试。手动更新失败保留旧缓存并标注错误，不清空可用版本。不轮询、不自动下载游戏包。条件请求可用 ETag，遇限流停止并显示重试时间；不要无限重试。[GitHub API 最佳实践](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api)。

建议首批下载管理最小闭环为：固定来源目录 → 已缓存版本/提交 → 用户选定资产 → 独立暂存下载 → 大小与摘要校验 → 本地下载记录。资源解包必须有大小/条目上限，拒绝路径逃逸、链接和覆盖已有目录；发现包内模组/声音后接入当前本地资源注册及实例约束。下载、安装、启用是三个不同状态，不应合并为“已可游玩”。

分支 ZIP 应记录解析出的 commit SHA；同一分支名随时变化，单存 master/main 无法定位用户下载的版本。模组合集内多个模组也不能强行映射成单个 MOD_INFO。已有世界的 mods.json 关系仍需保持；不要根据资源文件 version 字段自动断言能跨 DDA、BN、TLG 或跨版本使用。此处是结合现有保护模型的实施建议，不是上游自动兼容协议。

## 本次验证范围

已实际读取以上 GitHub 仓库元数据、最近发布 API、Catapult 固定提交清单和资源作者 README；未下载大型游戏或声音包，未安装、执行游戏或改变用户配置。来源可用性与发布时间可能变化，运行时应展示实际同步结果。
