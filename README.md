<div align="center">
  <img src="src/assets/glossa-logo.png" width="96" alt="Glossa logo" />
  <h1>Glossa</h1>
  <p><strong>你的语言，你的节奏</strong><br />专为 macOS 打造的轻量级 LLM 划词 / 截图翻译与语境生词沉淀工具</p>

[![Release](https://img.shields.io/github/v/release/conanxyz/glossa?include_prereleases)](https://github.com/conanxyz/glossa/releases)
[![CI](https://github.com/conanxyz/glossa/actions/workflows/ci.yml/badge.svg)](https://github.com/conanxyz/glossa/actions/workflows/ci.yml)
![macOS 14+](https://img.shields.io/badge/macOS-14%2B-222222?logo=apple)
[![GPL-3.0-only](https://img.shields.io/badge/license-GPL--3.0--only-blue)](LICENSE)
![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri)
![Rust](https://img.shields.io/badge/Rust-2024-CE422B?logo=rust)
![Svelte 5](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte)

[English](README.en.md) · [安装](#安装与首次运行) · [开发](#本地开发) · [贡献](CONTRIBUTING.md)
</div>

> 开发预览版。当前已完成本机 Apple Silicon 验证；Intel 构建由发布流水线提供，尚待真机验收。
> 本仓库初始化阶段尚未发布首个 GitHub Release，徽章与下载链接将在仓库上传及发布后生效。

## 看一眼 Glossa

![界面预览位置：翻译浮窗、阅读视图、生词侧栏、服务与生词本设置](docs/images/preview.svg)

以上是明确标注的截图占位图。正式发布前将替换为合成样例截图，不使用私人原文或账号信息。
当前生词候选使用可折叠的**左侧栏**，保持译文阅读位置；并非早期设计中的底部面板。

## Why Glossa?

| 日常场景 | Glossa 的处理方式 |
| --- | --- |
| 阅读途中遇到陌生段落 | `Option+D` 取词、`Option+S` 截图并本地 OCR；先核对原文，再按 `X` 或点击箭头翻译 |
| 网页、PDF 复制后标题和列表粘连 | 提示词引导模型还原段落，`markdown-it` + DOMPurify 展示安全的 Markdown；可切换纯文本 |
| 翻译很快，生词整理很慢 | 接收译文即展示；两段协议将生词 JSON 与正文分离，不等生词解析才显示译文 |
| 同一个词，在不同文章中意思不同 | 自动提炼 2–5 个候选，记录语境释义、原句和例句译文；重复收录合并为一词多境 |
| 不想每次查词都消耗 Token | 选中原文中的词，使用 macOS `NLTagger` 词形还原和系统词典补录；支持本机朗读 |
| 误选了助记词、私钥或 API Key | SensitiveGuard 在捕获文字进入界面与网络前做本地检测；显式放行仅作用于当前原文 |

译文首字延迟取决于网络和模型，排版恢复也可能出错。SensitiveGuard 检测在本机完成，覆盖常见英文 BIP-39 助记词、EVM/BTC/Solana 私钥格式和 API Key；它可能误报或漏报，不能替代用户检查。

生词本支持 B2–C2、习语、短语和技术术语候选、笔记、掌握状态、搜索、CSV / Markdown / Anki TSV 导出。
自动提炼可关闭；候选侧栏展开时保持译文阅读位置，同次任务中手动收起后不会被再次自动展开。

## 服务与隐私

支持自定义 OpenAI 兼容服务、DeepSeek、MiniMax（中国 / 国际区）、Gemini、OpenAI、Claude、Qwen、智谱、SiliconFlow、OpenRouter、有道智云及 macOS 系统词典。

- API Key 保存在系统 Keychain，不写入应用配置、历史 SQLite 或日志。
- 智谱 `glm-4-flash-250414`、SiliconFlow 部分开源模型和 Gemini 免费层可作为入门选项，具体模型上下线、免费政策、额度与区域限制以服务商为准。远程服务仍需自己的有效账号。
- OCR、语言识别、SensitiveGuard 和系统词典在本机执行。点击翻译后，原文会发送给你选择的服务商。
- 历史记录与截图保存在本机，可设置保留期或关闭历史；生词本独立保存。系统词典依赖已安装并启用的词典。
- 默认服务置顶，自定义服务置底；默认自定义服务仍置顶。服务页支持中文、英文及拼音搜索。

## 安装与首次运行

1. 在 [GitHub Releases](https://github.com/conanxyz/glossa/releases) 中选择已发布版本：Apple Silicon（M 系列）使用 `aarch64`，Intel 使用 `x64` / `x86_64`。若尚无 Release，请先从源码构建。
2. 下载 `.dmg`，将 `Glossa.app` 拖到“应用程序”。Release 同时提供 `.app.tar.gz` 和 SHA-256 校验文件。
3. 打开 Glossa，在设置中填写服务、模型和密钥，并运行连接测试；离线系统词典无需密钥。
4. 在「系统设置 → 隐私与安全性」为 Glossa 授予辅助功能和屏幕录制权限。新版 macOS 的辅助功能授权可能显示为“设备控制和数据访问”。

最低要求 **macOS 14**。项目原生桥接与 Tauri 配置都以 14 为下限，尚未支持 macOS 13。

### 未公证开发包

当前使用 Ad-hoc 签名，尚未经过 Apple 公证。若提示“无法验证开发者”，优先使用「隐私与安全性 → 仍要打开」。
若可信的官方开发包因隔离属性被提示“应用已损坏”，先核对 Release 的 SHA-256 与来源，再自行执行：

```sh
sudo xattr -rd com.apple.quarantine /Applications/Glossa.app
```

这会移除该应用的下载隔离标记，不会让它获得 Apple 公证；不要用于来源不明或校验不符的文件。
Ad-hoc 更新可能使原有 Keychain / 捕获权限关联失效，需要重新允许同一应用。

## 本地开发

安装 Xcode Command Line Tools、Rust（rustup）和 Node.js 24 / npm、Python 3.11+；Rust 版本由 `rust-toolchain.toml` 固定。
使用 npm 与已提交的 lockfile，避免混用包管理器产生不同依赖树。

```sh
npm ci
npx playwright install chromium
npm run tauri -- dev
```

```sh
./scripts/verify.sh          # 安全卫生、fmt、Clippy、Rust 测试、前端检查/构建及浏览器测试
sh scripts/test-native.sh    # 本机词形还原、系统词典与合成图 OCR 检查
npm run bundle:mac          # 为当前架构构建发布版 .app 与 .dmg
sh scripts/build-app.sh     # 紧凑的本机调试 .app，输出至 artifacts/build.noindex
```

验证缓存位于 `artifacts/verify.noindex`，不会进入 Git 或 Spotlight；可在验证结束后删除，不影响已安装应用和用户数据。
`npm run dev` 只启动前端，不能独立执行原生捕获功能。

## 工程与发布

```text
src/                 Svelte 界面与交互
src-tauri/           桌面窗口、IPC、快捷键与生命周期
crates/core/         翻译流、SensitiveGuard、生词规则、SQLite
crates/platform/     macOS 捕获、OCR、词典与 Keychain
scripts/             一键自检、安全审计、构建与发布辅助
```

`main` 推送和 PR 在 `macos-latest` 运行完整自检。`v*` 标签或手动 Release 工作流在自检通过后，分别构建 Apple Silicon 和 Intel 安装包并上传至 **Draft Release**，由维护者验收后发布。无需手工上传安装包。
自动化已配置，但只有推送至 GitHub 后才能验证托管运行结果；创建仓库后需把 `macOS verification` 设为分支保护的必需检查。

请阅读 [贡献指南](CONTRIBUTING.md)、[AI 编码约束](AGENTS.md)、[安全报告流程](SECURITY.md) 和 [第三方声明](THIRD_PARTY_NOTICES.md)。
本地只读参考目录 `Easydict/`、用户数据、密钥、测试截图与打包产物均不纳入版本控制。

## License

Copyright © 2026 Glossa Contributors.

Glossa 项目代码按 **GNU General Public License v3.0 only (`GPL-3.0-only`)** 发布，完整条款见 [LICENSE](LICENSE)。
第三方依赖、字体、词表及服务商标志保留各自许可证与商标权；详细来源见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
