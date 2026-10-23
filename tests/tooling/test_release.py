import copy
import hashlib
import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('release', ROOT / 'scripts/release.py')
RELEASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RELEASE)
TAG = 'v0.1.0'
REPO = 'fixture/local'
SHA = 'a' * 40


def draft(**fields):
    return dict(id=42, tag_name=TAG, draft=True, prerelease=True,
                name='Custom title', body='Release notes written by the author',
                target_commitish=SHA, assets=[], **fields)


class FakeGitHub:
    def __init__(self, release=None, tag_sha=None):
        self.release = copy.deepcopy(release)
        self.tag_sha = tag_sha
        self.calls = []

    def __call__(self, resource, *, method='GET', payload=None, paginate=False):
        self.calls.append((resource, copy.deepcopy(payload)))
        if '/git/matching-refs/' in resource:
            return [[{'ref': f'refs/tags/{TAG}'}]] if self.tag_sha else [[]]
        if '/commits/' in resource:
            return {'sha': self.tag_sha}
        if resource.endswith('/releases?per_page=100'):
            return [[], [copy.deepcopy(self.release)]] if self.release else [[]]
        if resource.endswith('/releases') and method == 'POST':
            self.release = {'id': 42, 'assets': [], **payload}
        elif payload:
            self.release.update(payload)
        return copy.deepcopy(self.release)

    def publication_calls(self):
        return [call for call in self.calls if call[1] and call[1].get('draft') is False]


def installer_fixture():
    files, groups = {}, {}
    for target, suffixes in RELEASE.TARGETS.items():
        archive = f'Yurelay_{TAG}_{target}.app.tar.gz'
        dmg = f'Yurelay_{TAG[1:]}_{suffixes[0]}.dmg'
        manifest = f'SHA256SUMS-{target}.txt'
        files[archive] = f'synthetic app archive: {target}'.encode()
        files[dmg] = f'synthetic installer: {target}'.encode()
        files[manifest] = ''.join(
            f'{hashlib.sha256(files[name]).hexdigest()}  {"./" if name == dmg else ""}{name}\n'
            for name in [archive, dmg]
        ).encode()
        groups[manifest] = (archive, dmg)
    release = draft()
    release['assets'] = [{'id': index + 1, 'name': name, 'state': 'uploaded', 'size': len(content)}
                         for index, (name, content) in enumerate(files.items())]
    return release, files, groups


class ReleaseTests(unittest.TestCase):
    def prepare(self):
        return RELEASE.prepare_release(TAG, REPO, SHA)

    def test_new_release_stays_a_private_prerelease_draft(self):
        api = FakeGitHub()
        with patch.object(RELEASE, 'github_json', api):
            self.assertEqual(self.prepare(), '42')
        self.assertTrue(api.release['draft'])
        self.assertTrue(api.release['prerelease'])
        self.assertEqual(api.release['target_commitish'], SHA)
        self.assertEqual(api.publication_calls(), [])

    def test_existing_draft_is_found_on_later_page_and_pinned_without_changing_notes(self):
        release = draft()
        release['target_commitish'] = 'main'
        release['prerelease'] = False
        api = FakeGitHub(release)
        with patch.object(RELEASE, 'github_json', api):
            self.assertEqual(self.prepare(), '42')
        self.assertEqual(api.release['target_commitish'], SHA)
        self.assertEqual(api.release['body'], release['body'])
        self.assertEqual(api.release['name'], release['name'])
        self.assertFalse(api.release['prerelease'])
        self.assertTrue(api.release['draft'])
        self.assertEqual(api.calls[-1][1], {'target_commitish': SHA})

    def test_existing_published_release_is_never_overwritten(self):
        release = draft()
        release['draft'] = False
        api = FakeGitHub(release)
        with patch.object(RELEASE, 'github_json', api):
            with self.assertRaisesRegex(ValueError, 'already public'):
                self.prepare()
        self.assertTrue(all(payload is None for _, payload in api.calls))

    def test_existing_tag_must_match_build_commit(self):
        for sha in [SHA, 'b' * 40]:
            api = FakeGitHub(draft(), tag_sha=sha)
            with self.subTest(sha=sha), patch.object(RELEASE, 'github_json', api):
                if sha == SHA:
                    self.assertEqual(self.prepare(), '42')
                else:
                    with self.assertRaisesRegex(ValueError, 'build commit'):
                        self.prepare()
            self.assertEqual(api.publication_calls(), [])

    def test_api_failure_does_not_create_a_second_release(self):
        with patch.object(RELEASE, 'github_json', side_effect=ValueError('API unavailable')) as api:
            with self.assertRaisesRegex(ValueError, 'API unavailable'):
                self.prepare()
            api.assert_called_once()

    def test_duplicate_drafts_are_rejected(self):
        with patch.object(RELEASE, 'github_json', side_effect=[[[]], [[draft(), draft()]]]):
            with self.assertRaisesRegex(ValueError, 'Multiple releases'):
                self.prepare()

    def publish(self, release, files, *, corrupt=False, tag_sha=None, change_assets=False):
        api = FakeGitHub(release, tag_sha)

        def download(repository, tag, directory, names):
            self.assertEqual((repository, tag), (REPO, TAG))
            self.assertEqual(set(names), set(files))
            for name in names:
                content = files[name]
                if corrupt and name.endswith('.dmg'):
                    content += b'corrupted'
                (directory / name).write_bytes(content)
            if change_assets:
                api.release['assets'][0]['id'] += 100

        with patch.object(RELEASE, 'github_json', api), patch.object(
            RELEASE, 'download_assets', side_effect=download,
        ) as downloader:
            try:
                RELEASE.publish_release(TAG, REPO, SHA, '42')
            finally:
                self.last_api = api
                self.last_downloader = downloader
        return api

    def test_both_architectures_and_checksums_are_verified_before_publication(self):
        for prerelease in [False, True]:
            release, files, _ = installer_fixture()
            release['prerelease'] = prerelease
            with self.subTest(prerelease=prerelease):
                api = self.publish(release, files)
                self.assertFalse(api.release['draft'])
                self.assertEqual(api.release['body'], release['body'])
                self.assertEqual(api.release['name'], release['name'])
                self.assertEqual(api.release['prerelease'], prerelease)
                self.assertEqual(api.publication_calls(), [
                    (f'repos/{REPO}/releases/42', {'draft': False}),
                ])

    def test_missing_or_incomplete_architecture_assets_prevent_publication(self):
        for state in ['missing', 'uploading', 'empty']:
            release, files, _ = installer_fixture()
            asset = next(a for a in release['assets'] if a['name'].endswith('_x64.dmg'))
            if state == 'missing':
                release['assets'].remove(asset)
            elif state == 'uploading':
                asset['state'] = 'uploading'
            else:
                asset['size'] = 0
            with self.subTest(state=state), self.assertRaisesRegex(ValueError, 'Incomplete installer'):
                self.publish(release, files)
            self.assertEqual(self.last_api.publication_calls(), [])
            self.last_downloader.assert_not_called()

    def test_corrupted_download_prevents_publication(self):
        release, files, _ = installer_fixture()
        with self.assertRaisesRegex(ValueError, 'Checksum mismatch'):
            self.publish(release, files, corrupt=True)
        self.assertTrue(self.last_api.release['draft'])
        self.assertEqual(self.last_api.publication_calls(), [])

    def test_missing_checksum_entry_prevents_publication(self):
        release, files, _ = installer_fixture()
        manifest = next(name for name in files if name.endswith('.txt'))
        files[manifest] = files[manifest].splitlines(keepends=True)[0]
        with self.assertRaisesRegex(ValueError, 'Missing installer checksums'):
            self.publish(release, files)
        self.assertEqual(self.last_api.publication_calls(), [])

    def test_unsafe_or_duplicate_manifest_paths_are_rejected(self):
        release, files, _ = installer_fixture()
        manifest = next(name for name in files if name.endswith('.txt'))
        good = files[manifest]
        for bad in [b'f' * 64 + b'  ../../private\n', good + good]:
            with self.subTest(manifest=bad):
                files[manifest] = bad
                with self.assertRaisesRegex(ValueError, 'Invalid installer checksum'):
                    self.publish(release, files)
                self.assertEqual(self.last_api.publication_calls(), [])

    def test_changed_draft_target_or_tag_prevents_publication(self):
        for change in ['target', 'tag']:
            release, files, _ = installer_fixture()
            if change == 'target':
                release['target_commitish'] = 'b' * 40
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.publish(release, files, tag_sha='b' * 40 if change == 'tag' else None)
            self.assertEqual(self.last_api.publication_calls(), [])

    def test_replaced_assets_after_download_prevent_publication(self):
        release, files, _ = installer_fixture()
        with self.assertRaisesRegex(ValueError, 'assets changed'):
            self.publish(release, files, change_assets=True)
        self.assertEqual(self.last_api.publication_calls(), [])

    def test_download_failure_keeps_the_draft_private(self):
        release, _, _ = installer_fixture()
        api = FakeGitHub(release)
        with patch.object(RELEASE, 'github_json', api), patch.object(
            RELEASE, 'download_assets', side_effect=ValueError('Download failed'),
        ):
            with self.assertRaisesRegex(ValueError, 'Download failed'):
                RELEASE.publish_release(TAG, REPO, SHA, '42')
        self.assertEqual(api.publication_calls(), [])

    def test_cli_rejects_bad_context_without_emitting_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'output'
            output.write_text('')
            result = subprocess.run(
                ['python3', str(ROOT / 'scripts/release.py'), 'prepare'],
                env={'PATH': '/usr/bin:/bin', 'RELEASE_TAG': 'v0.1.0;bad',
                     'GITHUB_REPOSITORY': REPO, 'GITHUB_SHA': SHA,
                     'GITHUB_OUTPUT': str(output)},
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output.read_text(), '')


if __name__ == '__main__':
    unittest.main()
