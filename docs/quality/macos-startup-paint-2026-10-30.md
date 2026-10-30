# macOS 空白首屏：定位与原生回归

日期：2026-10-30；平台：本机 macOS 27.0.1（26A434）Apple Silicon；基线 e7eceb5。

## 重现及因果对照

在独立 identifier `com.cataclysmhub.qa.blank-window` 的 Debug QA 包中重现，不使用玩家元数据。此包仅通过构建命令临时覆盖 productName/identifier，仓库正式标识保持不变；未改签名或系统授权。QA 实际数据库通过 lsof 确认位于 `~/Library/Application Support/com.cataclysmhub.qa.blank-window/metadata.sqlite`。

直接运行包内 Hub 程序，原生窗口和截图均为空白，单次窗口交互后可显示。两次后台诊断以及一次 Finder 双击诊断复现该现象。

临时观测 setup 起止、页面加载、前端模块、React layout effect 提交和 requestAnimationFrame：

- 空白样本 setup 13:27:42.400 开始、13:27:42.826 结束；页面于 13:27:43.113 加载完成。
- 模块于 13:27:43.132 执行，React 于 13:27:43.152 提交；未交互阶段无首帧回调，原生截图仍白。
- 只增加 macOS setup 最后一次 `main.set_focus()` 后，对照样本 React 于 13:29:10.822 提交、首帧于 13:29:10.840 回调，原生界面无需点击即显示。

原始观测日志见 [修复前](evidence/macos-startup-paint-2026-10-30/before-background.txt) 与 [聚焦对照](evidence/macos-startup-paint-2026-10-30/after-focus.txt)，日志时间为 UTC，本文转换为北京时间。Finder 初始对照没有控制台日志，依据原生 AX 和截图。

这些证据排除该样本的数据初始化阻塞与前端未提交，定位到未激活窗口的绘制阶段；一次聚焦可消除当前复现。没有证明所有 WebKit 空白页均由此引起。

## 最小修复

仅在 macOS setup 完成后取得主窗口并调用一次 `set_focus()`；不增加定时器、自动重载、后台节流关闭、JS 补丁、新依赖或数据库迁移。不在轮询或游戏运行时反复抢焦点。启动时激活主窗口是本次行为变化，Hub 不是后台常驻服务。

Tauri 本机锁定版本 2.12.5 的 `WebviewWindow::set_focus` 委托原生窗口聚焦；官方 [窗口聚焦 API](https://docs.rs/tauri/2.12.5/tauri/webview/struct.WebviewWindow.html#method.set_focus)提供该能力。本次没有把改 WebView 节流策略作为修复前提。

诊断命令、前端探针、on_page_load 日志均已从产品代码移除；证据日志仅保存在文档目录。此次 UI 生命周期行为没有合适的无原生窗口单测接缝，不添加只检查调用字符串的测试；以原生重现和对照作为回归证据。

## 干净构建验收

移除全部诊断后重新构建同 identifier QA 包：

1. 后台运行包内程序，首次 AX 即包含完整游玩环境首页和首次游玩按钮，无额外点击。
2. 退出 QA，刷新 Finder 文件引用后双击最终 QA 包，首次 AX 与截图均直接显示首页，无额外激活；设置导航响应正常，随后正常退出。
3. 没有启动游戏、创建实例、改配置、移动存档或更改安全属性。
4. 主力实例 121 文件和恢复实例 127 文件与此前完整路径/摘要清单一致，见 [保护范围核对](evidence/macos-startup-paint-2026-10-30/preserved-instances.json)。

干净 QA Debug 与普通标识 Debug 打包均成功，普通包 67.71 MiB。task check:rust 通过（格式、全特性 Clippy、309 项通过/4 项忽略）。普通包继续加载已有 QA 工作区进行后台启动检查：最初一次 AX 尚无 HTML，随后未执行任何窗口输入便自动出现三个独立环境，截图确认已绘制；因此修复的是需交互才能恢复的持续空白，不承诺消除所有加载期间的短暂白底。结论限于本机 Debug、直接启动及 Finder 正常双击；签名发行版、其他 macOS 版本和后台自启动服务尚未在此验证。


## 后续优化：启动底色与主题一致

在上述一次聚焦修复基础上，只增加两处静态设置：主窗口 `backgroundColor: #111715`（锁定 Tauri schema 明确作用于原生窗口和 WebView），HTML head 内同步声明 html/body 深色背景及 color-scheme，并将 theme-color 修正为同一色值。颜色与 styles.css 的最终 --bg 一致；不等待 JS 或外部 CSS 加载才设置底色。现有 CSP 允许 inline style，不放宽安全策略。

没有引入加载动画、隐藏窗口、额外延时或重载；独立目录参数、资源占用、启动摘要校验和存档保护全部保持。

同一独立 QA 标识重新打包后，实际执行：

- 后台直接运行包内 Hub 程序，最初 AX 尚无 HTML，随后截图已显示完整深色首页，无需点击恢复；截图采样未见白底。此截图没有停留在前端尚未绘制的瞬间，不能据此声称逐帧全程无任何闪烁。
- 退出后，在新临时目录复制同一 QA 包（保留原构建，绕过 Finder 对重建 inode 的缓存引用），从 Finder 双击实际包文件；首次 AX 和截图显示深色首页，无需额外激活，随后正常退出。
- 未启动游戏或更改系统授权。主力121文件、恢复127文件相对路径/摘要仍完全一致。

npm run check 通过：159 项前端测试、格式、类型与生产构建；独立 QA 与普通标识 Debug .app 均打包成功。底色属于低影响静态配置，没有添加镜像常量的单测，也没有重复后端全量测试；上一轮后端309项通过证据继续有效。本轮证据见 [主题验收记录](evidence/macos-startup-theme-2026-10-30/verification.json)。仍仅为本机 macOS Debug 与截图采样，未做连续高帧率录屏或其他系统兼容认证。
