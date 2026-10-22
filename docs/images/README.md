# README media

## Five-feature demos — 2026-10-23

The main READMEs display the language-matched 15-second demo directly below the introduction, in addition to the logo and badges. Earlier screenshots remain here as reference material and are no longer embedded in the READMEs.

| Language | GIF | MP4 |
| --- | --- | --- |
| Chinese | [yurelay-features-zh.gif](yurelay-features-zh.gif) | [yurelay-features-zh.mp4](yurelay-features-zh.mp4) |
| English | [yurelay-features-en.gif](yurelay-features-en.gif) | [yurelay-features-en.mp4](yurelay-features-en.mp4) |

- Both editions cover screenshot OCR, selection translation, the macOS dictionary, reply composition and automatic PDF line-break cleanup. Each feature gets 2.6 seconds, followed by a two-second brand ending.
- The Chinese opening uses “截取画面，识别文字。” The English edition uses English interface labels and localized examples.
- GIFs are 1280×720 at 15 fps, loop silently and are visible without expanding a section. A matching `*-poster.png` frame is selected when the reader prefers reduced motion. MP4 files are 1440×810 at 30 fps with music and UI sounds.
- Yurelay screens come from current Svelte components rendered with local synthetic fixtures. Capture rectangles and selection highlights on the sample document are explanatory graphics, **not native macOS capture recordings**. OCR, dictionary and model responses are preset examples, not quality or latency evidence.
- The footage contains no private documents, real credentials, provider traffic, Keychain records or user history. It does not validate native permissions, OCR, dictionary APIs or global shortcuts.
- Automatic cleanup specifically illustrates conservative joining of mid-sentence PDF line breaks and the Undo cleanup control; it does not claim grammar correction or arbitrary rewriting.
- The films were composed with huashu-motion using the original Yurelay logo and existing product colors. Chinese titles use Noto Sans SC; English titles use Outfit. Font licenses are SIL OFL; the small “Made with huashu-motion” end signature is retained. Only the exported media is included here, not the rendering runtime or local authoring environment.

## Earlier interface examples — 2026-10-22

The `readme-current-*` images retained here for reference show the current Svelte components rendered by local Playwright fixtures. They are **browser interface captures, not native macOS screenshots or generated UI illustrations**. All labels are Chinese. No real provider request, API key, Keychain entry, captured document or user history was used.

| File | What to look for |
| --- | --- |
| [readme-current-source.png](readme-current-source.png) | A source draft in reading mode, pencil edit control, Append and the D/S hint; no translation has been sent |
| [readme-current-translation-light.png](readme-current-translation-light.png) | Current compact reading window with the selected service, source and translation in light appearance |
| [readme-current-translation-dark.png](readme-current-translation-dark.png) | The same content and controls in dark appearance |
| [readme-current-reply.png](readme-current-reply.png) | Reply intent, preset English reply, expanded Chinese reference translation and Copy reply; this capture frames the reply panel |
| [readme-current-services.png](readme-current-services.png) | Default service, saved-key placeholder and direct saved-key authorization; endpoint and model are illustrative |
| [readme-current-prompts.png](readme-current-prompts.png) | Separate translation/reply prompt configuration |
| [readme-current-keychain.png](readme-current-keychain.png) | Yurelay's pre-authorization explanation, not a simulated macOS password prompt |
| [readme-current-vocabulary.png](readme-current-vocabulary.png) | Synthetic resilient entry, learning status and source context |
| [readme-current-history.png](readme-current-history.png) | Synthetic reading records, source filters and selected source/translation |

### Capture method and limits

- Reused the local fixtures from `tests/ui/workflow.spec.ts`, `tests/ui/disclosure.spec.ts` and `tests/ui/settings.spec.ts`, with authored example passages and preset results. The application components and styles were not changed for the screenshots.
- Used reduced motion and waited for fonts and the relevant rendered state. Compact-window viewport height follows the visible content; the reply image is a direct element screenshot. Pixels and text were not retouched.
- Source: a fictional request for draft feedback by Friday. Reply intent, translation and back-translation are authored examples, not claims about a model's quality. `demo-model` and `fixture.example` are placeholders, not recommended production settings.
- Saved-key status and authorization state are mocked. No system password dialog was photographed. These media cannot establish TCC, Keychain, OCR or global-hotkey behavior; those need separate native acceptance.
- The original screenshot captions explained the shown state. In particular, a ready draft is waiting for a fresh D/S press, and the back-translation is expanded for illustration even though it starts collapsed.

## Earlier native macOS captures

These original files are retained for reference. They show an earlier Tauri/WKWebView interface and are no longer the primary screenshots in the READMEs. Old action labels may differ from the current D/S workflow.

| File | Content |
| --- | --- |
| [readme-translation-light.png](readme-translation-light.png) | Earlier source, translation and vocabulary sidebar in light appearance |
| [readme-translation-dark.png](readme-translation-dark.png) | Earlier dark appearance with source-word lookup and vocabulary |
| [readme-vocabulary.png](readme-vocabulary.png) | Native lighthouse entry, personal note and two source contexts |
| [readme-history.png](readme-history.png) | Native sample history and selected source/translation |
| [readme-workflow.gif](readme-workflow.gif) | Edited native walkthrough: entered text → translation → source-word lookup → collection |

The earlier native captures used an isolated demo bundle and separate data directory, not the installed application or user database. Source sentences, notes and history were authored for the demo. No API keys were entered; unique demo profile identifiers avoided saved credentials. Translation came from a loopback sample-response server, while dictionary lookup, vocabulary collection, persistence, themes and history used the application's own implementation.

The GIF starts with text already entered; it does not demonstrate OS selection or screenshot capture and is not a latency benchmark. Native recording controls were cropped out; GIF frames were padded, resized and edited to show the main steps. Application content was not retouched or generated. The lighthouse and harbor examples are fictional; dictionary text came from the Mac's enabled dictionary.

## Earlier reading workflow PR evidence

`pr-reading-reply-light.png`, `pr-reading-narrow-dark.png`, `pr-reading-settings-simple.png` and `pr-reading-prompts-simple.png` came from earlier local Playwright fixtures. They contain synthetic source text, model labels, reply outputs and settings. They are layout evidence, not native permission or capture evidence. Different examples and rendering environments must not be treated as pixel or latency comparisons.
