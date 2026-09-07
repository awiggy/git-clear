<!-- lang:zh-CN -->
# Git Clear

Git Clear 是一款引导式界面的桌面 Git 客户端，使用 Rust 和 egui 构建。它把日常和高级 Git 操作组织成清晰的任务页面——项目、保存改动、远程同步、历史版本、分支、临时收起、合并冲突与设置——每一步都有说明文字和安全的默认值，让不熟悉 Git 命令行细节的人也能可靠地完成操作。

## 主要能力

- 管理多个工作空间与仓库：打开、创建、克隆，多项目标签
- 保存改动：暂存、提交、差异查看、文件时间追溯，安全的丢弃与恢复说明
- 远程同步：Fetch / Pull / Push，认证失败、无上游、分叉和冲突场景的分步引导
- 历史版本：提交图谱、搜索、比较、检出、回退、拣选与归档
- 分支与临时收起：本地/远程分支管理，Stash 完整流程
- 冲突与历史整理：三方冲突工具（含可选 AI 建议）、交互式变基
- 高级仓库能力：标签、补丁、Git LFS、Git Flow、子模块与子树、性能基准测试
- 设置与桌面工具：主题、字体、语言、SSH、提交身份、托管账号与可选 AI 模型

## 安装

从 [Releases](https://github.com/awiggy/git-clear/releases) 下载对应平台的安装包：

- **macOS**：`GitAgent-Clear-<版本>-macOS.dmg`（Apple Silicon/Intel 通用），打开后拖入应用程序文件夹
- **Windows**：`GitAgent-ClearSetup-<版本>.exe`，安装向导可选安装路径
- **Linux**：`GitAgent-Clear_<版本>_amd64.deb`，适用于 Debian / Ubuntu

安装后用户数据保存在各平台的标准位置：

```text
Windows: <安装路径>\data
macOS:   ~/Library/Application Support/Git Agent Clear
Linux:   $XDG_DATA_HOME/git-agent-clear 或 ~/.local/share/git-agent-clear
```

应用内提供"检查更新"，更新来源为本仓库的 Release。

## 构建

```bash
cargo test                  # 运行测试
cargo build --release       # 产出 Release 二进制，位于 target/release/
```

## 主题

`theme.json` 是界面默认色彩池。主题令牌使用 HSL 模板，`${c}` 会被替换为选中的强调色。应用优先加载可执行文件旁的 `theme.json`，找不到则回退到内置默认。

在 `theme.json` 旁创建 `theme.local.json` 可以在不改动默认文件的情况下自定义配色：只覆盖声明的键，其余继承。该文件已被 Git 忽略。设置 `GIT_AGENT_THEME` 可选择另一个基础配色文件。

## 发布

推送 `clear-v<版本>` 标签（版本号须与 `Cargo.toml` 一致）会自动运行测试、构建三个平台的安装包并发布 GitHub Release：

```bash
git tag clear-v1.2.1
git push origin clear-v1.2.1
```

以此方式构建的安装包会启用应用内自更新，更新源为本仓库的 Release 流。

