import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('repository_audit', Path(__file__).resolve().parents[2] / 'scripts/audit.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AuditTests(unittest.TestCase):
    def test_forbidden_paths_cover_nested_builds_and_database_sidecars(self):
        for path in ['Easydict/a.swift', 'crates/core/target/file', 'nested/.env.production', 'state.sqlite3', 'state.sqlite3-wal', 'state.db-shm', 'logs/service.log', 'x/Glossa.app/Contents/MacOS/glossa']:
            self.assertTrue(audit.bad_path(path), path)
        for path in ['Cargo.lock', 'package-lock.json', 'src/main.ts', 'src-tauri/build.rs', '.github/workflows/ci.yml']:
            self.assertFalse(audit.bad_path(path), path)

    def test_report_redacts_credentials(self):
        secret = b'sk-' + b'x' * 40
        issues = audit.problems('src/example.ts', b'const key = "' + secret + b'";')
        self.assertTrue(issues)
        self.assertNotIn(secret.decode(), str(issues))
        self.assertFalse(audit.problems('tests/example.ts', b'fixture-only-key'))

    def test_personal_paths_fail_but_relative_paths_pass(self):
        self.assertTrue(audit.problems('src/file.ts', b'/Users/' + b'fixture-user/project'))
        self.assertFalse(audit.problems('src/file.ts', b'assets/icon.png'))


if __name__ == '__main__':
    unittest.main()
