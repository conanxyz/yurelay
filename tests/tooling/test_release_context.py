import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('release_context', ROOT / 'scripts/release-context.py')
CONTEXT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONTEXT)


class ReleaseContextTests(unittest.TestCase):
    def resolve(self, event_name='release', event=None, sha='a' * 40):
        if event is None:
            event = {'action': 'published', 'release': {
                'tag_name': 'v0.1.0', 'id': 42, 'draft': False,
            }}
        return CONTEXT.resolve_release_id(event_name, event, 'v0.1.0', 'fixture/local', sha)

    def test_stable_and_prerelease_publication_reuse_event_id_without_api_calls(self):
        for prerelease in [False, True]:
            event = {'action': 'published', 'release': {
                'id': 42, 'tag_name': 'v0.1.0', 'draft': False,
                'prerelease': prerelease, 'name': 'Custom title', 'body': 'My release notes',
            }}
            with self.subTest(prerelease=prerelease), patch.object(CONTEXT, 'github_json') as api:
                self.assertEqual(self.resolve(event=event), '42')
                api.assert_not_called()
                self.assertEqual(event['release']['body'], 'My release notes')

    def test_invalid_publication_events_are_rejected(self):
        invalid = [
            {'action': 'created', 'release': {'id': 42, 'draft': False, 'tag_name': 'v0.1.0'}},
            {'action': 'published', 'release': {'id': 42, 'draft': True, 'tag_name': 'v0.1.0'}},
            {'action': 'published', 'release': {'id': 42, 'draft': False, 'tag_name': 'v0.0.1'}},
            {'action': 'published', 'release': {'id': True, 'draft': False, 'tag_name': 'v0.1.0'}},
        ]
        for event in invalid:
            with self.subTest(event=event), self.assertRaises(ValueError):
                self.resolve(event=event)

    def test_manual_build_finds_a_draft_on_a_later_page(self):
        pages = [[{'id': 1, 'tag_name': 'v0.0.1', 'draft': True}],
                 [{'id': 42, 'tag_name': 'v0.1.0', 'draft': True}]]
        with patch.object(CONTEXT, 'github_json', return_value=pages) as api:
            self.assertEqual(self.resolve('workflow_dispatch', {}), '42')
            api.assert_called_once_with('repos/fixture/local/releases?per_page=100')

    def test_manual_first_build_allows_draft_creation(self):
        with patch.object(CONTEXT, 'github_json', return_value=[[]]):
            self.assertEqual(self.resolve('workflow_dispatch', {}), '')

    def test_manual_upload_to_published_release_requires_the_tagged_source(self):
        releases = [[{'id': 42, 'tag_name': 'v0.1.0', 'draft': False}]]
        for sha in ['a' * 40, 'b' * 40]:
            with self.subTest(sha=sha), patch.object(
                CONTEXT, 'github_json', side_effect=[releases, [{'sha': 'a' * 40}]],
            ):
                if sha.startswith('a'):
                    self.assertEqual(self.resolve('workflow_dispatch', {}, sha), '42')
                else:
                    with self.assertRaisesRegex(ValueError, 'differs from this build'):
                        self.resolve('workflow_dispatch', {}, sha)

    def test_api_failure_does_not_fall_back_to_creating_another_release(self):
        with patch.object(CONTEXT, 'github_json', side_effect=ValueError('API unavailable')):
            with self.assertRaisesRegex(ValueError, 'API unavailable'):
                self.resolve('workflow_dispatch', {})

    def test_ambiguous_release_matches_are_rejected(self):
        matches = [[{'id': 42, 'tag_name': 'v0.1.0', 'draft': True},
                    {'id': 43, 'tag_name': 'v0.1.0', 'draft': True}]]
        with patch.object(CONTEXT, 'github_json', return_value=matches):
            with self.assertRaisesRegex(ValueError, 'Multiple releases'):
                self.resolve('workflow_dispatch', {})

    def test_cli_emits_a_release_id_only_after_validating_the_event(self):
        with tempfile.TemporaryDirectory() as directory:
            event_path = Path(directory) / 'event.json'
            output_path = Path(directory) / 'output'
            event = {'action': 'published', 'release': {
                'tag_name': 'v0.1.0', 'id': 42, 'draft': False,
            }}
            env = dict(os.environ, GITHUB_EVENT_NAME='release',
                       GITHUB_EVENT_PATH=str(event_path), GITHUB_OUTPUT=str(output_path),
                       RELEASE_TAG='v0.1.0')
            for tag, expected_code in [('v0.1.0', 0), ('v0.0.1', 1)]:
                with self.subTest(tag=tag):
                    event['release']['tag_name'] = tag
                    event_path.write_text(json.dumps(event))
                    output_path.write_text('')
                    result = subprocess.run(
                        ['python3', str(ROOT / 'scripts/release-context.py')],
                        env=env, capture_output=True, text=True, check=False,
                    )
                    self.assertEqual(result.returncode, expected_code)
                    self.assertEqual(output_path.read_text(),
                                     'release_id=42\n' if expected_code == 0 else '')


if __name__ == '__main__':
    unittest.main()
