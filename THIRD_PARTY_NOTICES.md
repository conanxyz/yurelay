# Third-party notices

Yurelay's own code is GPL-3.0-only. This does not relicense dependencies or provider
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
Yurelay does not implement wallet generation. The bundled list remains attributed
to its upstream authors.

Generated inventories include versions, declared licenses, source links and
license/notice files present in local package archives. Some registry archives
omit full license files; these entries are explicitly identified, not presented
as independently verified legal notices. Review upstream sources when changing
or redistributing those dependencies. Regenerate with
`python3 scripts/third-party-notices.py` after `npm ci` and Rust dependency fetch.

Provider names and marks identify supported services and do not imply endorsement.
The project logo is an AI-assisted Y monogram adapted, with maintainer approval,
from the maintainer-supplied artwork. Easydict is a local
read-only reference, excluded from Git; its source and assets are not distributed.

GSAP was removed before the initial commit because its custom license carries
use restrictions. Yurelay's entrance effects now use native Web Animations and
IntersectionObserver, with reduced-motion support.

## KaTeX 0.19.0 and Commander

KaTeX is loaded locally on demand; its fonts are bundled with the application. No CDN is used. KaTeX and its Commander dependency use the MIT license (compatible with GPL-3.0-only). The license distributed with the KaTeX package, including its bundled fonts, is retained below.

The MIT License (MIT)

Copyright (c) 2013-2020 Khan Academy and other contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

### Commander

(The MIT License)

Copyright (c) 2011 TJ Holowaychuk <tj@vision-media.ca>

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
'Software'), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED 'AS IS', WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
