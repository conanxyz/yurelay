<div align="center">
  <img src="src/assets/glossa-logo.png" width="88" alt="Glossa" />
  <h1>Glossa</h1>
  <p><strong>你的语言，你的节奏</strong><br />专为 macOS 打造的轻量级 LLM 划词 / 截图翻译与语境生词沉淀工具。</p>

![macOS 14+](https://img.shields.io/badge/macOS-14%2B-222222?logo=apple)
[![GPL-3.0-only](https://img.shields.io/badge/license-GPL--3.0--only-blue)](LICENSE)
![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri)

[English](README.en.md) · [下载安装](https://github.com/conanxyz/glossa/releases) · [参与贡献](CONTRIBUTING.md)
</div>

读懂眼前的内容，也把值得记住的表达留下来。Glossa 将划词、截图翻译和语境生词本放在同一个轻量窗口里，让阅读与积累自然衔接。

![界面预览占位：翻译窗口、阅读视图、生词侧栏、服务与生词本设置](docs/images/preview.svg)

## ✨ 核心特性

### ⚡ 随手取词，边译边读

选中文字或框选屏幕区域，用全局快捷键唤起 Glossa。截图文字在本机识别；你可以先检查、编辑原文，再按 `X` 或点击箭头开始翻译，译文随返回内容逐步显示。

阅读模式保留标题、列表与代码块，并帮助还原 PDF、网页和更新日志中粘连的段落。需要原始文字时，一键切换纯文本，无需重新翻译。

### 📖 把生词留在语境里

翻译时可自动提炼 2–5 个值得学习的单词、短语或习语，附上当前语境中的含义。候选词在左侧栏展示，保持译文阅读位置；同一次任务中手动收起后，不会被再次自动展开。

- **两种收录方式**：一键保存候选词，或直接划选原文中的单词，通过本机词形还原与系统词典补录，不消耗模型 Token。
- **一词多境**：再次遇到同一个词时，累积不同文章中的原句、释义与出现频次。
- **方便复习**：系统朗读、掌握状态、个人笔记，以及 **CSV / Markdown / Anki TSV** 导出。

### 🛡️ 敏感内容，先在本机检查

SensitiveGuard 在捕获文字进入结果窗口和发送给模型前，本地检测常见的英文 BIP-39 助记词、私钥与 API Key 格式。命中规则时隐藏明文并阻止发送，未放行的内容不写入历史。

服务密钥保存在 **macOS 系统钥匙串（Keychain）**。OCR 和系统词典查询在本机完成；远程翻译只发送给你选择的服务商。历史记录可设置保留期，也可关闭。

检测可能误报或漏报，请在发送前检查原文。模型生成的译文、排版和释义也需要结合上下文判断。

### 🔌 选择适合自己的服务

无需账号即可使用 macOS 系统词典查词，也可以接入自己的模型服务。Glossa 预置智谱 `glm-4-flash-250414` 等模型候选，支持利用服务商提供的免费模型或免费层起步；具体费用、额度与可用性以各平台政策为准。

| 服务 | 配置方式 |
| :--- | :--- |
| macOS 系统词典 | 无需密钥，使用本机已启用的词典；适合单词与短语查询 |
| 智谱 AI、硅基流动、DeepSeek、通义千问 | API Key + 模型名称 |
| Gemini、OpenAI、Claude | 对应平台的 API Key + 模型名称 |
| MiniMax | 选择中国区或国际区，填写对应区域的 API Key |
| OpenRouter | API Key + 模型 ID |
| 有道智云 | 应用 ID + 应用密钥 |
| 自定义服务 | OpenAI 兼容地址 + 模型名称，按服务要求填写密钥 |

## 🚀 安装与首次配置

### 1. 安装 Glossa

前往 [Releases](https://github.com/conanxyz/glossa/releases)，下载适合芯片架构的 `.dmg`，将 Glossa 拖入“应用程序”：

- **Apple Silicon（M 系列）**：选择 `aarch64`。
- **Intel**：选择 `x64` / `x86_64`。

需要 **macOS 14 或更新版本**。若 Releases 暂无安装包，可按文末说明从源码构建。

<details>
<summary>首次打开提示“无法验证开发者”或“应用已损坏”？</summary>

未公证的开发包可能触发 macOS 提示。优先在「系统设置 → 隐私与安全性」选择“仍要打开”。

如果确认下载自项目 Releases、校验文件一致，且问题由下载隔离属性引起，可执行：

```sh
sudo xattr -rd com.apple.quarantine /Applications/Glossa.app
```

此命令只移除该应用的隔离标记，不代表 Apple 公证；不要用于来源不明或校验不符的文件。

</details>

### 2. 开启捕获权限

在「系统设置 → 隐私与安全性」中允许 Glossa 使用：

- **辅助功能（Accessibility）**：读取其他应用中选中的文字。
- **屏幕录制（Screen Recording）**：截取选区并识别文字。

也可从 Glossa「设置 → 翻译与快捷键」检查状态并打开对应系统设置。新版 macOS 的辅助功能授权可能显示为“设备控制和数据访问”。

### 3. 配置第一个服务

点击菜单栏 Glossa 图标 → **设置… → 服务**。

- **先试本地查词**：选择“系统词典”，设为默认，无需 API Key。
- **开始段落翻译**：选择智谱或其他服务，填写自己的 API Key，选择模型，点击“测试连接”；成功后设为默认并保存。智谱可从预置的 `glm-4-flash-250414` 开始。

远程服务需要有效账号、模型权限和可用额度。“测试连接”会发送固定示例文字，可能产生少量 API 费用。

## ⌨️ 日常使用与快捷键

| 操作 | 默认快捷键 / 方式 | 说明 |
| :--- | :--- | :--- |
| 划词取词 | `Option + D` | 在其他应用中选中文字后，捕获至 Glossa |
| 截图 OCR | `Option + S` | 框选区域，在本机识别文字 |
| 开始翻译 | `X` 或右侧箭头 | 检查原文后发起翻译；编辑文字时请点击箭头，`X` 会正常输入 |
| 切换阅读 / 纯文本 | 译文区切换按钮 | 改变显示方式，不重复请求模型 |
| 收录生词 | 候选词的添加按钮，或划选原文 | 保存语境，随时收起左侧栏继续阅读 |
| 导出生词 | 设置 → 生词本 → 导出 | 选择 CSV、Markdown 或 Anki TSV |
| 找回窗口 / 打开设置 | 菜单栏图标或 Dock | 切换其他应用不打断后台翻译 |

取词和截图快捷键可在「设置 → 翻译与快捷键」中修改；发生快捷键冲突时，应用会给出提示。

## 🛠️ 本地构建与贡献

欢迎反馈问题、改善翻译体验或提交代码。开发流程与架构约束见 [贡献指南](CONTRIBUTING.md)；安全问题请按 [安全报告流程](SECURITY.md) 私下反馈。

<details>
<summary>展开开发与构建命令</summary>

准备 Xcode Command Line Tools、Rust（rustup）、Node.js 24 和 Python 3.11+。项目会使用固定的 Rust 工具链。

```sh
npm ci                         # 安装锁定版本的依赖
npx playwright install chromium # 安装浏览器测试环境
npm run tauri -- dev            # 启动桌面开发模式
./scripts/verify.sh             # 运行完整本地自检
npm run bundle:mac              # 构建本机 .app 与 .dmg
```

</details>

## 📄 开源协议与致谢

Copyright © 2026 Glossa Contributors. 项目按 **GNU GPL v3.0 only（`GPL-3.0-only`）** 开源，完整条款见 [LICENSE](LICENSE)。

Glossa 的功能与交互设计参考了 [Easydict](https://github.com/tisfeng/Easydict) 和 [Bob](https://github.com/ripperhe/Bob)。开发过程中，也对照 Easydict 的划词、截图、OCR 与翻译服务接入实现进行了研究。感谢这些项目及其作者提供的思路与经验。

感谢 Tauri、Rust、Svelte 及相关开源社区。依赖、字体和服务商标志的许可见 [第三方声明](THIRD_PARTY_NOTICES.md)。
