#!/usr/bin/env python3
"""Regenerate dependency notices from the locked local dependency sources."""
import hashlib
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--format-version', '1', '--locked'], cwd=root))
lock = json.loads((root / 'package-lock.json').read_text())

def collect(packages, destination):
    notices = {}
    rows = []
    for name, version, license_id, directory, source in sorted(packages):
        references = []
        for f in sorted(directory.iterdir()):
            if f.is_file() and f.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'COPYRIGHT', 'NOTICE')):
                text = '\n'.join(line.rstrip() for line in f.read_text(errors='replace').splitlines()).strip()
                digest = hashlib.sha256(text.encode()).hexdigest()[:16]
                notices[digest] = text
                references.append(digest)
        if not references:
            references.append('Upstream package supplies license metadata only; see source URL above')
        rows.append(f'{name} {version} | {license_id} | {source}\nNotices: {", ".join(references)}')
    output = 'Generated dependency inventory and available notices. Do not remove upstream notices.\nSome registry archives omit license files; those entries link to the source package.\n\n'
    output += '\n\n'.join(rows)
    for digest, text in sorted(notices.items()):
        output += f'\n\n{"=" * 72}\nNOTICE {digest}\n{"=" * 72}\n{text}\n'
    (root / destination).write_text(output)

rust = [(p['name'], p['version'], p.get('license') or 'SEE NOTICE', Path(p['manifest_path']).parent,
         f"https://crates.io/crates/{p['name']}/{p['version']}")
        for p in metadata['packages'] if p['source']]
collect(rust, 'licenses/RUST-NOTICES.txt')
node = []
for path, p in lock['packages'].items():
    if path and not p.get('dev') and (root / path).is_dir():
        data = json.loads((root / path / 'package.json').read_text())
        node.append((data['name'], data['version'], str(data.get('license', 'SEE NOTICE')),
                     root / path, f"https://www.npmjs.com/package/{data['name']}/v/{data['version']}"))
collect(node, 'licenses/NPM-NOTICES.txt')
print(f'Collected notices for {len(rust)} Rust and {len(node)} production npm packages.')
