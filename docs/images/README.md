# README media

The `readme-*` files are captures of the native macOS application (Tauri / WKWebView), not browser mockups or generated UI illustrations.

| File | Content |
| --- | --- |
| `readme-translation-light.png` | Source, translation and vocabulary sidebar in light appearance |
| `readme-translation-dark.png` | Dark appearance with a source-word lookup and collected vocabulary |
| `readme-vocabulary.png` | A sample word, personal note and two source contexts |
| `readme-history.png` | Sample history records and the selected record's source and translation |
| `readme-workflow.gif` | Review entered text → request translation → select a source word → collect |

## Capture setup

- An isolated demo bundle and separate data directory were used. The production bundle identifier, installed application and existing user database were not used for these media.
- The source sentences, notes and initial history records were authored for this demonstration. No API keys were entered. Unique demo profile identifiers avoid accessing saved provider credentials.
- Translation was served by a loopback sample-response server. Native dictionary lookup, vocabulary collection, local persistence, theme settings and history views ran through the application's own implementation.
- The GIF starts with text already entered. It does not demonstrate OS selection capture or screenshot capture and must not be presented as a remote-model latency benchmark.
- Images were cropped to remove native title-bar recording controls. Frames were padded and resized for the GIF, which is edited to show the main interaction steps. Application content was not retouched or generated.
- The README offers a static alternative for reduced-motion preferences. All media currently use the Chinese interface.

The lighthouse and harbor examples are sample content, not real user reading history. Local dictionary text in the word detail comes from the Mac's enabled dictionary.
