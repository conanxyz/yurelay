#!/usr/bin/env python3
"""Validate tag input without evaluating it as shell code."""
import json
import os
import re
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parent.parent
package = json.loads((root / 'package.json').read_text())
tauri = json.loads((root / 'src-tauri/tauri.conf.json').read_text())
cargo = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']
tag = os.environ['REQUESTED_TAG']
expected = 'v' + package['version']
if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', tag) or tag != expected:
    raise SystemExit('Release tag must exactly match the package version.')
if not package['version'] == tauri['version'] == cargo['version']:
    raise SystemExit('Cargo, npm and Tauri versions differ.')
with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
    output.write(f'tag={tag}\n')
print(f'Validated {tag}')
