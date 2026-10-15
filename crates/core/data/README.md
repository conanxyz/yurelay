# Bundled local data

`bip39-english.txt` is the 2,048-word English BIP-39 list from
https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt
(BIP-39, MIT: https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki).
The list is embedded into the executable; checking never uses a network service.

`translation-prompt.txt` is Glossa's shared built-in prompt. The service editor
imports the same file that Rust embeds, so the preview cannot drift from the template.
