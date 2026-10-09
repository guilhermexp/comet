# Zeron

在本地管理你的编码 agent（Claude Code、Codex、Cursor、Devin、Grok、Hermes、Pi、Antigravity、OMP、Prime Agent 以及其他本地 CLI runtime），也可以打开多设备同步。

*[English](README.md) | 简体中文 | [한국어](README.ko.md) | [日本語](README.ja.md)*

![Zeron 桌面应用](docs/media/readme/app-screenshot.jpg)

## 桌面应用

此仓库是保留 Workers 和 OMP 功能的 Comet fork，请从源码构建。上游 Zeron 桌面版本可从 [GitHub Releases](https://github.com/zeronsh/zeron/releases/latest) 下载对应平台的最新版本：

- **macOS** — `zeron-<version>-macos-arm64.dmg`
- **Windows** — `zeron-<version>-windows-x86_64-setup.exe`
- **Linux** — `zeron-<version>-linux-<arch>.tar.gz`，解压后运行里面的 `install.sh`

不用账号，也不用联网，会话就存在这台设备上。此 fork 仅在配置自己的 release feed 后启用应用更新。

## 无界面运行（CLI）

适用于服务器等没有显示器的机器，比如在你合上笔记本之后继续跑 agent 的 VPS。此 fork 请从源码构建并运行 `zeron headless`。以下 Linux 安装器安装的是上游 Zeron：

```bash
curl -fsSL https://zeron.sh/install.sh | sh
zeron status
```

安装脚本会把引擎作为后台服务拉起来，重启之后也会自己回来。

```bash
zeron status      # 查看本地/同步模式和引擎状态
zeron update      # 更新到最新版本
zeron daemon start|stop|restart|status
```

## 多设备同步（可选）

登录后，可以在一台设备上起 agent，换另一台设备接着看、接着操作：

```bash
zeron daemon stop
zeron login        # 或者 zeron logout 切回纯本地模式
zeron daemon start
```

登录同一账号的设备可以读写彼此工作区里的文件，所以只登录你信任的设备。已有的本地会话不会被上传。

## 桌面工作流

桌面端应用将受管的 Orchestrator 对话与本地 Workers 会话区分开来。主窗口关闭后，Workers 仍然保持活跃，并可随时通过 macOS 菜单栏重新打开。

在任意模式下，`Details` 会显示 Workspace、结构化的 To-dos（如果可用）以及账号 Usage，而 `Files` 则使用 Material 文件图标浏览当前选中的本地 checkout。打开文件时对话仍然可见，并复用与 Terminal 和 Git 相同的侧边栏；它支持只读预览 Markdown、源码、HTML、PDF、图片、CSV/TSV 以及 Excel 工作簿。托管在其他设备上的 checkout 文件必须在对应的宿主设备上打开。

---

想参与开发，或者好奇它怎么跑起来的？[Ask DeepWiki](https://deepwiki.com/zeronsh/zeron)，也可以看 [ARCHITECTURE.md](ARCHITECTURE.md)。

采用 [MIT License](LICENSE)。
