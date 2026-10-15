# Third-party notices

Glossa's own code is GPL-3.0-only. This does not relicense dependencies or provider
trademarks. Keep this document and `licenses/` with redistributed bundles.

| Component | License / source |
| --- | --- |
| Tauri and plugins | MIT OR Apache-2.0; selected under MIT where offered |
| Rust dependencies | See `licenses/RUST-NOTICES.txt` and Cargo.lock; for dual licenses, a GPLv3-compatible permissive option is used |
| Svelte, markdown-it and production npm dependencies | See `licenses/NPM-NOTICES.txt` and package-lock.json |
| DOMPurify | MPL-2.0 OR Apache-2.0; selected under Apache-2.0 |
| Outfit font | SIL OFL-1.1; `licenses/OFL-Outfit.txt`, [source](https://github.com/Outfitio/Outfit-Fonts) |
| LobeHub provider SVGs | MIT; `licenses/MIT-LobeHub.txt` and `src/assets/providers/README.md` |
| English BIP-39 word list | MIT; [BIP-39 specification and authors](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki), [word list](https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt); used solely for local detection |

BIP-39 authors: Marek Palatinus, Pavol Rusnak, Aaron Voisine and Sean Bowe.
Glossa does not implement wallet generation. The bundled list remains attributed
to its upstream authors.

Generated inventories include versions, declared licenses, source links and
license/notice files present in local package archives. Some registry archives
omit full license files; these entries are explicitly identified, not presented
as independently verified legal notices. Review upstream sources when changing
or redistributing those dependencies. Regenerate with
`python3 scripts/third-party-notices.py` after `npm ci` and Rust dependency fetch.

Provider names and marks identify supported services and do not imply endorsement.
The project logo is the maintainer-supplied Glossa asset. Easydict is a local
read-only reference, excluded from Git; its source and assets are not distributed.

GSAP was removed before the initial commit because its custom license carries
use restrictions. Glossa's entrance effects now use native Web Animations and
IntersectionObserver, with reduced-motion support.
