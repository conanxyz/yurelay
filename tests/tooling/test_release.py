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
        if method == 'DELETE':
            asset_id = int(resource.rsplit('/', 1)[1])
            self.release['assets'] = [a for a in self.release['assets'] if a['id'] != asset_id]
            return None
        if '/git/matching-refs/' in resource:
            return [[{'ref': f'refs/tags/{TAG}'}]] if self.tag_sha else [[]]
        if '/commits/' in resource:
            return {'sha': self.tag_sha}
        if resource.endswith('/releases?per_page=100'):
            return [[], [copy.deepcopy(self.release)]] if self.release else [[]]
        if resource.endswith('/releases') and method == 'POST':
            self.release = {'id': 42, 'assets': [], **payload}
        elif payload:
            if self.release['draft'] and 'tag_name' not in payload:
                self.release['tag_name'] = 'untagged-fixture'
            self.release.update(payload)
        return copy.deepcopy(self.release)

    def publication_calls(self):
        return [call for call in self.calls if call[1] and call[1].get('draft') is False]


def installer_fixture():
    files, groups = {}, {}
    for target, suffixes in RELEASE.TARGETS.items():
        dmg = f'Yurelay_{TAG[1:]}_{suffixes[0]}.dmg'
        manifest = f'SHA256SUMS-{target}.txt'
        files[dmg] = f'synthetic installer: {target}'.encode()
        files[manifest] = ''.join(
            f'{hashlib.sha256(files[name]).hexdigest()}  {"./" if name == dmg else ""}{name}\n'
            for name in [dmg]
        ).encode()
        groups[manifest] = (dmg,)
    release = draft()
    release['assets'] = [{'id': index + 1, 'name': name, 'state': 'uploaded', 'size': len(content)}
                         for index, (name, content) in enumerate(files.items())]
    return release, files, groups


class ReleaseTests(unittest.TestCase):
    def prepare(self):
        return RELEASE.prepare_release(TAG, REPO, SHA)

    def test_new_release_defaults_to_stable_but_stays_private(self):
        api = FakeGitHub()
        with patch.object(RELEASE, 'github_json', api):
            self.assertEqual(self.prepare(), '42')
        self.assertTrue(api.release['draft'])
        self.assertFalse(api.release['prerelease'])
        self.assertEqual(api.release['name'], f'Yurelay {TAG}')
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
        self.assertEqual(api.release['tag_name'], TAG)
        self.assertEqual(api.calls[-1][1], {'tag_name': TAG, 'target_commitish': SHA})

    def test_existing_published_release_is_never_overwritten(self):
        release = draft()
        release['draft'] = False
        api = FakeGitHub(release)
        with patch.object(RELEASE, 'github_json', api):
            with self.assertRaisesRegex(ValueError, 'already public'):
                self.prepare()
        self.assertTrue(all(payload is None for _, payload in api.calls))

    def test_explicit_prerelease_draft_is_preserved_when_preparing(self):
        release = draft()
        release['target_commitish'] = 'main'
        api = FakeGitHub(release)
        with patch.object(RELEASE, 'github_json', api):
            self.assertEqual(self.prepare(), '42')
        self.assertTrue(api.release['prerelease'])
        self.assertTrue(api.release['draft'])
        self.assertEqual(api.publication_calls(), [])

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

    def test_asset_deletion_accepts_an_empty_204_response(self):
        result = subprocess.CompletedProcess([], 0, stdout='', stderr='')
        with patch.object(RELEASE.subprocess, 'run', return_value=result):
            self.assertIsNone(RELEASE.github_json(
                f'repos/{REPO}/releases/assets/99', method='DELETE',
            ))

    def test_duplicate_drafts_are_rejected(self):
        with patch.object(RELEASE, 'github_json', side_effect=[[[]], [[draft(), draft()]]]):
            with self.assertRaisesRegex(ValueError, 'Multiple releases'):
                self.prepare()

    def publish(self, release, files, *, corrupt=False, tag_sha=None, change_assets=False):
        api = FakeGitHub(release, tag_sha)

        def download(repository, tag, directory, names):
            self.assertEqual((repository, tag), (REPO, TAG))
            self.assertTrue(set(names).issubset(files))
            for name in names:
                content = files[name]
                if corrupt and name.endswith('.dmg'):
                    content += b'corrupted'
                (directory / name).write_bytes(content)
            if change_assets:
                api.release['assets'][0]['id'] += 100

        def upload(repository, tag, manifest):
            self.assertEqual((repository, tag, manifest.name), (REPO, TAG, 'SHA256SUMS.txt'))
            content = manifest.read_bytes()
            files[manifest.name] = content
            api.release['assets'] = [a for a in api.release['assets']
                                     if a['name'] != manifest.name]
            api.release['assets'].append({
                'id': 1000, 'name': manifest.name, 'size': len(content), 'state': 'uploaded',
                'digest': 'sha256:' + hashlib.sha256(content).hexdigest(),
            })

        with patch.object(RELEASE, 'github_json', api), patch.object(
            RELEASE, 'download_assets', side_effect=download,
        ) as downloader, patch.object(RELEASE, 'upload_manifest', side_effect=upload) as uploader:
            try:
                RELEASE.publish_release(TAG, REPO, SHA, '42')
            finally:
                self.last_api = api
                self.last_downloader = downloader
                self.last_uploader = uploader
        return api

    def test_default_release_is_published_as_stable_and_latest_after_verification(self):
        api = FakeGitHub()
        with patch.object(RELEASE, 'github_json', api):
            self.prepare()
        packaged, files, _ = installer_fixture()
        release = api.release
        release['assets'] = packaged['assets']
        published = self.publish(release, files).release
        self.assertFalse(published['draft'])
        self.assertFalse(published['prerelease'])
        self.assertEqual(published['make_latest'], 'true')

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
                self.assertEqual(api.release['tag_name'], TAG)
                self.assertEqual(api.publication_calls(), [
                    (f'repos/{REPO}/releases/42', {
                        'tag_name': TAG, 'target_commitish': SHA, 'draft': False,
                        'make_latest': 'false' if prerelease else 'true',
                    }),
                ])
                self.assertEqual({a['name'] for a in api.release['assets']}, {
                    'Yurelay_0.1.0_aarch64.dmg', 'Yurelay_0.1.0_x64.dmg', 'SHA256SUMS.txt',
                })
                self.assertEqual(len(files['SHA256SUMS.txt'].splitlines()), 2)

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
        files[manifest] = b''
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

    def test_legacy_archives_and_architecture_manifests_are_removed_before_publication(self):
        release, files, _ = installer_fixture()
        existing = {asset['name'] for asset in release['assets']}
        for index, name in enumerate(sorted(RELEASE.legacy_asset_names(TAG) - existing)):
            release['assets'].append({'id': 50 + index, 'name': name,
                                      'state': 'uploaded', 'size': 10})
        api = self.publish(release, files)
        self.assertEqual({a['name'] for a in api.release['assets']}, {
            'Yurelay_0.1.0_aarch64.dmg', 'Yurelay_0.1.0_x64.dmg', 'SHA256SUMS.txt',
        })
        self.assertEqual(api.calls[-1], (f'repos/{REPO}/releases/42', {
            'tag_name': TAG, 'target_commitish': SHA, 'draft': False,
            'make_latest': 'false',
        }))

    def test_retry_after_partial_cleanup_uses_combined_checksums(self):
        release, files, _ = installer_fixture()
        combined = b''.join(content for name, content in files.items() if name.endswith('.txt'))
        files['SHA256SUMS.txt'] = combined
        release['assets'] = [a for a in release['assets'] if not a['name'].startswith('SHA256SUMS-')]
        release['assets'].append({'id': 90, 'name': 'SHA256SUMS.txt',
                                  'state': 'uploaded', 'size': len(combined)})
        api = self.publish(release, files)
        self.assertFalse(api.release['draft'])
        self.assertEqual(len(api.release['assets']), 3)

    def test_unknown_attachment_is_not_deleted_or_published(self):
        release, files, _ = installer_fixture()
        release['assets'].append({'id': 99, 'name': 'custom-document.pdf',
                                  'state': 'uploaded', 'size': 10})
        with self.assertRaisesRegex(ValueError, 'Unexpected release attachments'):
            self.publish(release, files)
        self.assertEqual(self.last_api.publication_calls(), [])
        self.last_downloader.assert_not_called()
        self.assertIn('custom-document.pdf', [a['name'] for a in self.last_api.release['assets']])

    def test_corrupt_installer_does_not_remove_legacy_assets(self):
        release, files, _ = installer_fixture()
        old = {'id': 99, 'name': 'Yurelay_v0.1.0_aarch64-apple-darwin.app.tar.gz',
               'state': 'uploaded', 'size': 10}
        release['assets'].append(old)
        with self.assertRaisesRegex(ValueError, 'Checksum mismatch'):
            self.publish(release, files, corrupt=True)
        self.assertIn(old, self.last_api.release['assets'])
        self.last_uploader.assert_not_called()

    def test_combined_manifest_upload_failure_prevents_publication(self):
        release, files, _ = installer_fixture()
        api = FakeGitHub(release)

        def download(repository, tag, directory, names):
            for name in names:
                (directory / name).write_bytes(files[name])

        with patch.object(RELEASE, 'github_json', api), patch.object(
            RELEASE, 'download_assets', side_effect=download,
        ), patch.object(RELEASE, 'upload_manifest', side_effect=ValueError('Upload failed')):
            with self.assertRaisesRegex(ValueError, 'Upload failed'):
                RELEASE.publish_release(TAG, REPO, SHA, '42')
        self.assertEqual(api.publication_calls(), [])
        self.assertEqual(api.release['assets'], release['assets'])

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
