# FAQ and usage details

[简体中文](FAQ_cn.md) · [Back to the product overview](../README.md)

This guide covers interaction details, permissions, data retention and service configuration.

[Installation and permissions](#install) · [Reading and replies](#workflow) · [Vocabulary](#vocabulary) · [History](#history) · [Services](#services) · [Shortcuts and common questions](#shortcuts) · [Privacy and Keychain](#privacy)

<a id="install"></a>

## 🚀 Install and get started

Requires **macOS 14 or newer**. When a package is published, download its `.dmg` from [Releases](https://github.com/conanxyz/yurelay/releases), open it and drag Yurelay into Applications. Choose `aarch64` for Apple Silicon or `x64` / `x86_64` for Intel. If no package is available yet, follow the [build instructions in the README](../README.md#development). A Homebrew installation channel is not yet available.

1. Allow **Accessibility** for text selection and **Screen Recording** for screenshots.
2. Choose the system dictionary or configure a model in Settings → Services.
3. Capture some text; once it is ready, press `D` / `S` or click Translate / Look up.
4. If a saved key needs authorization, choose Authorize and continue and follow the macOS prompt.

### Getting started, settings and models

First launch offers **Permissions → Service → Practice**. Grant only the permissions you need, or choose **Set up later**. Completed or skipped setup does not reopen on every launch. Use the title-bar **gear**, app menu, tray menu or `⌘,` for settings. The **question mark** and **Settings → Getting started** replay the guide. Clicking the title-bar question mark again performs the same persistent skip as Set up later and returns to reading without clearing your draft. The empty reader no longer repeats a setup banner.

Click the service/model name in the translation window to open that service's settings. Save model changes explicitly; change shortcuts under **Translation**. System dictionary handles words and phrases, translation services handle passages, and generative model services also support context explanations and replies. Configuration is separate from verification: a connection test sends a fixed sample only when requested.

Practice uses a separate demo. It makes no network requests and never replaces your source, clipboard or history. It displays your currently configured shortcuts.

The main window defaults to 520 × 560 macOS logical pixels. Content, guide and vocabulary changes never resize it; overflow scrolls. Manual sizes persist across launches, and zoom restores the previous normal bounds. In narrow windows vocabulary appears below the reading content without covering it or changing the reading column width.

The title-bar pin keeps the window on top and prevents automatic hiding when another app gains focus. Unpinning restores your existing auto-hide preference. Esc and the explicit Hide button still hide a pinned window.

The guide offers a Zhipu quick setup: register with BigModel, create a standard model API key, paste it and choose Save and use. The free `glm-4.7-flash` model and endpoint are preset. Coding Plan keys use a different endpoint. Existing model configurations are preserved, and advanced settings remain available.

### First launch and permissions

The current workflow produces ad-hoc-signed development builds without Apple notarization. After confirming a trusted source, follow [Apple's instructions](https://support.apple.com/en-us/102445) to use **Open Anyway** in System Settings → Privacy & Security. If macOS reports a damaged file, check the file and release source first.

Accessibility enables selection from other apps; Screen Recording enables screenshots. Typing, pasting and local dictionary lookup do not require those two permissions. They are separate from signing, notarization and Keychain access. After updating a development build, consult Yurelay's permission recovery guide if needed.

Drag the handle below either reading area to adjust its height, or focus it and use the arrow keys. Wide windows keep a 24px inset and fill the available space with aligned source and translation panels. Either handle adjusts both columns together; their shared proportion is saved separately from compact-window heights. Compact windows stack the panels and retain independent adjustments. Expanding Write / Reply adds space below without shrinking the reader. Double-click or press Home to restore the default fit; manual proportions survive collapse, retranslation and restart. Long text scrolls inside a manually sized area; the outer window stays unchanged.

<a id="workflow"></a>

## Read and reply

1. **Capture text.** Select it, take a screenshot, or type and paste directly.
2. **Review the source.** Text opens in reading mode; use the pencil to edit and check OCR or cleanup results.
3. **Start translation.** Wait for the source, release the shortcut, then press `D` / `S` alone or click Translate / Look up.

Capture or OCR in progress, held-key repeats, editable inputs and input-method composition do not trigger translation. Reading mode supports paragraphs, headings, lists and code blocks. You can also switch to plain text and copy the translation or Markdown separately.

Switching apps hides the result window by default; disable this in Appearance settings if preferred. Hiding does not interrupt an active translation, and you can return to the source app to capture again.

### Organize source text and structure

- **Append captures:** Combine up to ten selections or screenshots in one draft. Editing or switching drafts pauses append mode.
- **Clean up line breaks:** Conservative PDF line-break cleanup runs locally after capture by default. Undo is available when the text changed; this is not grammar correction or arbitrary rewriting.
- **Read tables and formulas:** Apply a conservative table layout when OCR coordinates support it and compare the original image. Formulas render locally with a plain-source fallback. The optional structure setting asks the model to preserve columns, numbers, units and recognizable formulas; it cannot recover symbols missed by OCR.

### Explain context and compose replies

Select a passage to explicitly generate a contextual explanation. To respond, open Write / Reply, describe your intent in your own language and choose Generate reply. The result shows Original and Translation directly. Original is the generated reply in the source language using the current service; Translation uses the current translation language. The reply should preserve names, facts and commitments in your reply intent; example answers in the original message are not your identity or intended response. The reference translates the generated reply, not the entire original message.

Explanations and replies can each be stopped, regenerated and revisited in history independently. The copy icon in the top-right of the reply card copies only Original, excluding the translation, labels and history. It never pastes or sends automatically. Changing the source or intent marks the old reply as stale until you explicitly regenerate it.

<a id="vocabulary"></a>

## 📚 Dictionary lookup and contextual vocabulary

**Looking up a word?** Set **macOS system dictionary** as the default in Settings → Services to query enabled local dictionaries without an API key. With a model service selected, Yurelay chooses dictionary or passage instructions based on the input.

For a sentence or paragraph, the system dictionary offers configured translation services for the current query. Choose one, then press `D` / `S` or click Translate / Look up to translate; your default service stays the same.

**Want to remember an expression?** Click or select an English source word for native lemmatization and a dictionary preview, then click **Add**. You can also collect words suggested by the model.

- Keep multiple source sentences and available meanings under one word; adding the same sentence again does not increase its count.
- Add personal notes, track Learning / Mastered status and search words or contexts.
- In Read source, learning words and short phrases can be highlighted locally. Click one to revisit saved contexts or mark it mastered; matching does not collect it again or increase its count.
- New candidates do not automatically open the sidebar. Opening it preserves the source reading position, and late data does not reopen a manually collapsed sidebar.
- Use UK / US pronunciation and slower playback when matching system voices are available.
- Export **CSV, Markdown or Anki TSV** for further study.

<a id="history"></a>

## 🕘 Revisit what you have read

In **Settings → History**, search source or translation text and filter selections, screenshots and typed text. Review the source, edited text, saved screenshot and translation, copy content or reopen the result window.

History defaults to **7 days**. Choose a custom duration, keep it forever or disable disk saving. Explicitly collected vocabulary is saved separately for longer-term study.

Explanations and replies follow their parent record’s retention; standalone results follow the same duration setting. Assistant history keeps up to 40 operations within 32 MiB, with five previous attempts per operation. Sensitive results explicitly allowed for one request stay in memory. Restarting never resends unfinished requests.

<a id="services"></a>

## 🔌 Bring your own translation service

| Service | Setup |
| :--- | :--- |
| macOS system dictionary | No key; uses enabled local dictionaries |
| Zhipu, SiliconFlow, DeepSeek, Qwen | API key + model name |
| Gemini, OpenAI, Claude | Provider API key + model name |
| MiniMax | Choose China or international region and its matching key |
| OpenRouter | API key + model ID |
| Youdao | Application ID + application secret |
| Custom service | OpenAI-compatible endpoint + model; key if required |

Open Settings → Services, choose a provider, configure it, test the connection, then set it as default and save. Presets include models such as Zhipu's `glm-4-flash-250414`; pricing, free allowances and availability depend on the provider. Connection tests send fixed sample text and may incur a charge.

### Automatic fallback and prompt settings

Connection verification is bound to the saved endpoint, model and credential revision, and expires after seven days. Automatic fallback is off by default. If enabled, all configured compatible services are eligible, with at most three attempts before any result text appears. Connection tests report status and are not a prerequisite for fallback. This can incur charges at more than one provider; explicitly allowed sensitive requests do not fall back automatically.

New translations start with the default service saved in Settings. Results show the service that actually produced them, including an automatic fallback. Fallback does not change your default or the primary service chosen for that request. There is no separate next-service selector in the main reading window.

Translation and reply prompts have separate editors in Settings → Prompts. Services inherit them by default and can use a custom override. Existing per-service translation prompts are retained. Save changes to apply them; existing requests are not resent automatically.

<a id="shortcuts"></a>

## ⌨️ Shortcuts and interaction details

| Action | Default shortcut / entry point |
| :--- | :--- |
| Capture selected text | `Option + D` |
| Screenshot OCR | `Option + S` |
| Translate / expand or collapse translation | `D` / `S` or the arrow |
| Hide the result window | `Esc`; collapses an open vocabulary sidebar first |
| Reopen / settings | Menu bar icon or Dock |
| Export vocabulary | Settings → Vocabulary → Export |

### Interaction details and FAQ

- **Can I change the shortcuts?** Click any selection, screenshot or optional append shortcut button and press a key combination. Release the main key to finish, or press Esc to cancel. While recording an optional append binding, choose Clear and disable or press Delete to clear it. Recording and clearing take effect only after saving; conflicts are reported. The local second-step keys remain `D` / `S`.
- **Do D and S translate while I am typing?** In an input field, both keys type normally. Click the arrow or move focus outside the field before using the shortcut.
- **Can I hold D / S until capture completes?** No. Release and press again after the source is ready. `Option` / `Alt`, `Control` or `Command` combinations, input-method composition and editable fields do not submit a translation.
- **What happens if I press capture twice?** Duplicate requests are ignored during capture. The selection shortcut preserves the result while Yurelay has focus. A new screenshot can start after the current capture finishes or is cancelled.
- **What can I collect?** The source must be detected as English or explicitly set to English. A preview is saved only after clicking Add. Clicking or selecting translation text does not preview or collect vocabulary.
- **What limits apply to automatic extraction?** Enable it in settings for English source text. Inputs over 150 words or 800 characters skip automatic extraction; translation remains available.
- **Will a collapsed sidebar reopen?** Later model metadata does not reopen a manually collapsed sidebar. Deliberately selecting another source word can open its preview.
- **Does reading history call the model again?** No. A new request requires an explicit Translate again action.
- **What does history cleanup remove?** Expiry is based on creation time. Saving a shorter retention period removes expired records; disabling disk saving clears existing history. Deleting a record permanently removes its source, screenshot and translation after confirmation. Collected vocabulary is managed separately.
- **Do the Sentence / Dictionary mode buttons change translation mode?** In service settings, these buttons switch the default prompt preview. The input determines the actual processing mode.

<a id="privacy"></a>

## 🛡️ Privacy and security

Before captured text reaches the result window or a remote model, SensitiveGuard checks common English BIP-39 seed phrases, private-key formats and API keys. Matches are hidden and blocked from sending, and unreleased content is not saved to history. Detection can miss secrets or produce false positives; OCR and model output also need contextual judgment.

Service keys are encrypted in **macOS Keychain**, never stored in application settings or the history database. Startup reads silently without repeated authorization prompts. When you explicitly translate and a saved key needs authorization, Yurelay first explains its Keychain storage. Choose **Authorize and continue** to let macOS request access and resume translation without re-entering the API key. **Authorize saved key** is also available in service settings.

If macOS asks for a password, it is usually your **Mac login password (keychain password)**, not your API key. The system authorization dialog handles that password. Already-authorized access may need no prompt; each service has its own Keychain item, and a new development signature may require authorization again. Saving or deleting a key may also request access; cancelling authorization preserves the original key and settings. Local OCR, dictionary lookup and speech do not require a remote model; remote translation sends the source to your chosen provider. Do not include keys or private captures in issue reports. See [SECURITY.md](../SECURITY.md) for security reports.

Cancelling Authorize and continue preserves the saved key and sends no translation for that request.
