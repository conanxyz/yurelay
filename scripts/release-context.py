#!/usr/bin/env python3
"""Resolve an existing release ID so asset uploads preserve release metadata."""
import json
import os
import re
import subprocess
from pathlib import Path


def github_json(resource):
    result = subprocess.run(
        ['gh', 'api', '--paginate', '--slurp', resource],
        capture_output=True, text=True, check=False,
    )
    if result.returncode:
        raise ValueError('Could not read GitHub release metadata; refusing to upload.')
    return json.loads(result.stdout)


def resolve_release_id(event_name, event, tag, repository, source_sha):
    if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', tag):
        raise ValueError('Invalid release tag.')
    if event_name == 'release':
        release = event.get('release', {})
        if event.get('action') != 'published' or release.get('draft') is not False:
            raise ValueError('Expected a published release event.')
        if release.get('tag_name') != tag:
            raise ValueError('Release event tag does not match the validated version.')
    elif event_name == 'workflow_dispatch':
        if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
            raise ValueError('Invalid repository name.')
        pages = github_json(f'repos/{repository}/releases?per_page=100')
        matches = [release for page in pages for release in page
                   if release.get('tag_name') == tag]
        if not matches:
            return ''  # Let tauri-action create an unpublished draft.
        if len(matches) != 1:
            raise ValueError('Multiple releases match this tag; refusing to choose one.')
        release = matches[0]
        if release.get('draft') is False:
            commits = github_json(f'repos/{repository}/commits/{tag}')
            if not source_sha or commits[0].get('sha') != source_sha:
                raise ValueError(
                    'The published tag differs from this build. Rerun its original release workflow.'
                )
    else:
        raise ValueError('Unsupported release workflow event.')
    release_id = release.get('id')
    if type(release_id) is not int or release_id <= 0:
        raise ValueError('Expected a positive release ID.')
    return str(release_id)


def main():
    try:
        event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
        release_id = resolve_release_id(
            os.environ['GITHUB_EVENT_NAME'], event, os.environ['RELEASE_TAG'],
            os.environ.get('GITHUB_REPOSITORY', ''), os.environ.get('GITHUB_SHA', ''),
        )
    except (KeyError, ValueError, TypeError, OSError) as error:
        raise SystemExit(f'Release context validation failed: {error}') from None
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write(f'release_id={release_id}\n')


if __name__ == '__main__':
    main()
