#!/usr/bin/env python3
"""Keep installers private until both architectures and checksums are ready."""
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

TARGETS = {
    'aarch64-apple-darwin': ('aarch64', 'arm64'),
    'x86_64-apple-darwin': ('x64', 'x86_64'),
}
DEFAULT_NOTES = (
    'macOS 14 or later. Choose aarch64 for Apple Silicon or x64 for Intel.\n'
    'These builds use ad-hoc signing and are not Apple-notarized.\n'
    'GPL-3.0-only. Source code is available at this release tag.'
)


def github_json(resource, *, method='GET', payload=None, paginate=False):
    args = ['gh', 'api', resource, '--method', method]
    if paginate:
        args.extend(['--paginate', '--slurp'])
    if payload is not None:
        args.extend(['--input', '-'])
    result = subprocess.run(args, input=json.dumps(payload) if payload is not None else None,
                            capture_output=True, text=True, check=False)
    if result.returncode:
        raise ValueError('GitHub release request failed; publication was not confirmed.')
    return json.loads(result.stdout)


def validate_inputs(tag, repository, source_sha):
    if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', tag):
        raise ValueError('Invalid release tag.')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise ValueError('Invalid repository name.')
    if not re.fullmatch(r'[0-9a-f]{40}', source_sha):
        raise ValueError('Expected the full build commit SHA.')


def validate_draft(release, tag, release_id=None):
    if type(release.get('id')) is not int or release['id'] <= 0:
        raise ValueError('Expected a positive release ID.')
    if release_id is not None and release['id'] != release_id:
        raise ValueError('Release ID changed.')
    if release.get('tag_name') != tag:
        raise ValueError('Release tag does not match the validated version.')
    if release.get('draft') is not True:
        raise ValueError('Release is already public; refusing to replace its packages.')


def verify_tag_source(repository, tag, source_sha):
    pages = github_json(f'repos/{repository}/git/matching-refs/tags/{tag}', paginate=True)
    matches = [ref for page in pages for ref in page if ref.get('ref') == f'refs/tags/{tag}']
    if not matches:
        return False
    if len(matches) != 1:
        raise ValueError('Ambiguous release tag.')
    commit = github_json(f'repos/{repository}/commits/{tag}')
    if commit.get('sha') != source_sha:
        raise ValueError('Release tag does not point to the build commit.')
    return True


def prepare_release(tag, repository, source_sha):
    validate_inputs(tag, repository, source_sha)
    verify_tag_source(repository, tag, source_sha)
    pages = github_json(f'repos/{repository}/releases?per_page=100', paginate=True)
    matches = [release for page in pages for release in page if release.get('tag_name') == tag]
    if len(matches) > 1:
        raise ValueError('Multiple releases match this tag; refusing to choose one.')
    if matches:
        release = matches[0]
        validate_draft(release, tag)
        if release.get('target_commitish') != source_sha:
            # Pin the draft, leaving its title, notes and prerelease flag untouched.
            release = github_json(f"repos/{repository}/releases/{release['id']}",
                                  method='PATCH', payload={'target_commitish': source_sha})
    else:
        release = github_json(f'repos/{repository}/releases', method='POST', payload={
            'tag_name': tag, 'target_commitish': source_sha,
            'name': f'Yurelay {tag}', 'body': DEFAULT_NOTES,
            'draft': True, 'prerelease': True,
        })
    validate_draft(release, tag)
    return str(release['id'])


def required_assets(release, tag):
    assets = release.get('assets', [])
    names = [asset.get('name') for asset in assets]
    if len(names) != len(set(names)):
        raise ValueError('Duplicate release asset names.')
    available = {asset['name'] for asset in assets
                 if asset.get('state') == 'uploaded' and asset.get('size', 0) > 0}
    groups = {}
    for target, suffixes in TARGETS.items():
        archive = f'Yurelay_{tag}_{target}.app.tar.gz'
        checksums = f'SHA256SUMS-{target}.txt'
        dmgs = [f'Yurelay_{tag[1:]}_{suffix}.dmg' for suffix in suffixes]
        found = [name for name in dmgs if name in available]
        if archive not in available or checksums not in available or len(found) != 1:
            raise ValueError(f'Incomplete installer assets for {target}; keeping the draft private.')
        groups[checksums] = (archive, found[0])
    return groups


def download_assets(repository, tag, directory, names):
    args = ['gh', 'release', 'download', tag, '--repo', repository, '--dir', str(directory)]
    for name in sorted(names):
        args.extend(['--pattern', name])
    result = subprocess.run(args, capture_output=True, text=True, check=False)
    if result.returncode:
        raise ValueError('Could not download every installer for verification; keeping the draft private.')


def verify_checksums(directory, groups):
    for manifest, expected in groups.items():
        seen = set()
        for line in (directory / manifest).read_text().splitlines():
            match = re.fullmatch(r'([0-9a-fA-F]{64}) [ *](?:\./)?([^/\\]+)', line)
            if not match or match[2] not in expected or match[2] in seen:
                raise ValueError('Invalid installer checksum manifest; keeping the draft private.')
            name = match[2]
            with (directory / name).open('rb') as asset:
                actual = hashlib.file_digest(asset, 'sha256').hexdigest()
            if actual != match[1].lower():
                raise ValueError(f'Checksum mismatch for {name}; keeping the draft private.')
            seen.add(name)
        if seen != set(expected):
            raise ValueError('Missing installer checksums; keeping the draft private.')


def publish_release(tag, repository, source_sha, release_id):
    validate_inputs(tag, repository, source_sha)
    if not release_id.isdecimal() or int(release_id) <= 0:
        raise ValueError('Expected a positive release ID.')
    resource = f'repos/{repository}/releases/{release_id}'
    release = github_json(resource)
    validate_draft(release, tag, int(release_id))
    groups = required_assets(release, tag)
    names = set(groups) | {name for group in groups.values() for name in group}
    snapshot = {asset['name']: (asset.get('id'), asset['size'], asset.get('digest'),
                               asset.get('updated_at'))
                for asset in release['assets'] if asset['name'] in names}
    with tempfile.TemporaryDirectory() as directory:
        directory = Path(directory)
        download_assets(repository, tag, directory, names)
        verify_checksums(directory, groups)
    # Recheck the draft after downloads, including any tag created while building.
    release = github_json(resource)
    validate_draft(release, tag, int(release_id))
    required_assets(release, tag)
    current = {asset['name']: (asset.get('id'), asset['size'], asset.get('digest'),
                              asset.get('updated_at'))
               for asset in release['assets'] if asset['name'] in names}
    if current != snapshot:
        raise ValueError('Installer assets changed during verification; keeping the draft private.')
    tag_exists = verify_tag_source(repository, tag, source_sha)
    if not tag_exists and release.get('target_commitish') != source_sha:
        raise ValueError('Draft target changed during the build; keeping the draft private.')
    result = github_json(resource, method='PATCH', payload={'draft': False})
    if result.get('draft') is not False or result.get('id') != int(release_id):
        raise ValueError('Publication was not confirmed.')


def main():
    try:
        tag = os.environ['RELEASE_TAG']
        repository = os.environ['GITHUB_REPOSITORY']
        sha = os.environ['GITHUB_SHA']
        if sys.argv[1:] == ['prepare']:
            release_id = prepare_release(tag, repository, sha)
            with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
                output.write(f'release_id={release_id}\n')
        elif sys.argv[1:] == ['publish']:
            publish_release(tag, repository, sha, os.environ['RELEASE_ID'])
            print(f'Published {tag} after verifying both architecture installers.')
        else:
            raise ValueError('Expected prepare or publish.')
    except (KeyError, ValueError, TypeError, OSError) as error:
        raise SystemExit(f'Release validation failed: {error}') from None


if __name__ == '__main__':
    main()
