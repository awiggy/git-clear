# 简洁版真实凭据实机验收清单

日期：2026-09-06（创建）。目标：把审计中标注"环境依赖"的功能升级为实测结论。

应用：`/Users/shuke/Desktop/git中台/dist/Git Agent Clear.app`

隔离测试仓库（沿用审计环境）：

- 本地仓库：`/private/tmp/git-agent-parity-guided`
- 本机裸远程：`/private/tmp/git-agent-parity-remote.git`

## 本机前置检查（已执行）

| 检查项 | 结果 | 影响 |
|---|---|---|
| `gh` CLI | 未安装 | 不影响应用内验收；仅无法用命令行核对 PR |
| `~/.ssh` 密钥 | 无密钥 | SSH 推送项需要先导入或生成密钥 |
| ssh-agent | 未运行 | 验收时可测试应用内 SSH Agent 流程本身 |
| GPG 私钥 | 无 | GPG 签名提交项需要先有密钥 |
| 全局 git 身份 | 未配置 | 提交身份项正好从空白状态测起 |
| 隔离仓库与 Clear 应用 | 均在位 | 可直接开始 |

## 验收记录表

验收口径沿用 `docs/original-vs-guided-functional-audit.md`：`✅ 实操` 表示在应用里点击并核对真实外部状态变化。结果列填写：通过 / 失败原因 / 环境缺什么。

### A. 提交身份（无需外部账号）

- [ ] A1. 设置与电脑工具 → 连接与身份：填写姓名和邮箱，保存后在隔离仓库创建一次提交，用 `git log` 核对作者。
- [ ] A2. GPG 签名（需要密钥）：导入或生成 GPG 密钥后，在提交选项中启用签名，`git log --show-signature` 核对。

### B. SSH 连接（需要密钥）

- [ ] B1. 生成或导入 SSH 密钥（应用内 SSH 设置或本机 `ssh-keygen`）。
- [ ] B2. 通过应用加载 SSH Agent，确认不再每次询问密码。
- [ ] B3. 把公钥加到 Git 托管账号，用 SSH 地址克隆/推送一次到真实远程。

### C. 托管平台账号（需要登录 GitHub 等）

- [ ] C1. 设置与电脑工具 → 连接与身份：添加托管账号，确认账号匹配当前仓库远程。
- [ ] C2. 从同步页"打开远程网页"，浏览器应跳到正确仓库页。
- [ ] C3. 在测试仓库创建 Pull Request，网页端核对 PR 真实生成。
- [ ] C4. 认证失败恢复：故意用错误凭据操作一次，确认引导提示正确。

### D. AI 能力（需要 API Key）

- [ ] D1. 设置与电脑工具 → 可选 AI：填写 API 地址、Key、模型 ID，点连通测试。
- [ ] D2. 确认 Key 存入独立钥匙串服务 `Git Agent Clear`，重启应用后仍可用。
- [ ] D3. 在三栏合并工具中对真实冲突跑一次 AI 建议，核对建议、理由和应用结果。

### E. 自动更新（等待第一个 `clear-v*` Release）

- [ ] E1. 第一个 Clear Release 发布后，用本地构建的旧版本点"检查更新"，应发现新版本并只下载 `GitAgent-Clear-*` 资产。
- [ ] E2. 确认更新不会触碰原版 `Git Agent.app`。

## 完成后

把结果回填到本文件勾选框，并在 `docs/original-vs-guided-functional-audit.md` 的"实际操作记录"一节追加结论。
