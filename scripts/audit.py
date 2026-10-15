#!/usr/bin/env python3
"""Check publishable paths and obvious secrets without printing secret values.

This is a repository hygiene gate, not a guarantee that all secrets are detected.
Run a full secret scanner before publication too (see CONTRIBUTING.md).
"""
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOKEN = re.compile(rb'(?:sk-(?:ant-)?[A-Za-z0-9_-]{32,}|gh[pousr]_[A-Za-z0-9_]{36,}|AKIA[0-9A-Z]{16})')
PRIVATE = re.compile(rb'-----BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY-----\s+[A-Za-z0-9+/=]{20,}')
HOME = re.compile(rb'/(?:Users|home)/[A-Za-z0-9_.-]+/')
FORBIDDEN = {'Easydict', 'node_modules', 'target', 'dist', 'build', 'artifacts', '.svelte-kit', '.idea', 'test-results', 'playwright-report'}


def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args])


def bad_path(path):
    p = Path(path)
    return (bool(set(p.parts) & FORBIDDEN) or p.name.startswith('.env')
            or p.name == '.DS_Store' or any(x.endswith('.app') for x in p.parts)
            or bool(re.search(r'\.(?:db(?:[.-].*)?|sqlite.*)$|\.(?:log|pem|key|p12|pfx|dmg)$', p.name))
            or p.name in {'credentials.json', 'secrets.json'})


def problems(path, data):
    found = []
    if bad_path(path):
        found.append('excluded local/generated/private file')
    # Binary resources are not text; secret scanners handle entropy separately.
    if b'\x00' not in data[:8192]:
        for pattern, label in [(TOKEN, 'credential-shaped literal'), (PRIVATE, 'private key block'), (HOME, 'personal absolute path')]:
            for match in pattern.finditer(data):
                found.append(f'{label} at line {data[:match.start()].count(bytes([10])) + 1}')
    return found


def main():
    staged = '--staged' in sys.argv
    names = git('ls-files', '-z') if staged else git('ls-files', '-c', '-o', '--exclude-standard', '-z')
    failures = []
    paths = sorted(set(p.decode() for p in names.split(b'\x00') if p))
    for name in paths:
        path = ROOT / name
        if staged:
            data = git('show', ':' + name)
        elif path.is_file():
            if path.is_symlink():
                if not path.resolve().is_relative_to(ROOT):
                    failures.append((name, 'symlink leaves repository'))
                    continue
            data = path.read_bytes()
        else:
            continue
        failures.extend((name, issue) for issue in problems(name, data))
    package = json.loads((ROOT / 'package.json').read_text())
    tauri = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())
    if package['license'] != 'GPL-3.0-only' or tauri['bundle']['license'] != 'GPL-3.0-only':
        failures.append(('license metadata', 'SPDX identifier mismatch'))
    cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']
    if cargo.get('license') != 'GPL-3.0-only':
        failures.append(('Cargo.toml', 'workspace license mismatch'))
    for manifest in ['crates/core/Cargo.toml', 'crates/platform/Cargo.toml', 'src-tauri/Cargo.toml']:
        member = tomllib.loads((ROOT / manifest).read_text())['package']
        if member.get('license') != {'workspace': True}:
            failures.append((manifest, 'must inherit the workspace license'))
    if not package['version'] == tauri['version'] == cargo['version']:
        failures.append(('version metadata', 'npm, Tauri and Cargo versions differ'))
    for name, issue in failures:
        print(f'{name}: {issue}', file=sys.stderr)
    print(f'Audited {len(paths)} {"index" if staged else "source"} paths; {len(failures)} findings.')
    return bool(failures)


if __name__ == '__main__':
    sys.exit(main())
