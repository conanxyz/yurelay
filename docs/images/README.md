# README media

## Current interface examples — 2026-10-22

The `readme-current-*` images used in the English and Chinese READMEs show the current Svelte components rendered by local Playwright fixtures. They are **browser interface captures, not native macOS screenshots or generated UI illustrations**. All labels are Chinese. No real provider request, API key, Keychain entry, captured document or user history was used.

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
- Image captions in the READMEs explain the shown state. In particular, a ready draft is waiting for a fresh D/S press, and the back-translation is expanded for illustration even though it starts collapsed.

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
