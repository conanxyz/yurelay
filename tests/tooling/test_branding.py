import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class BrandingTests(unittest.TestCase):
    def test_rebrand_preserves_existing_data_and_credential_identity(self):
        config = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())
        self.assertEqual(config['identifier'], 'com.autodict.desktop')
        platform = (ROOT / 'crates/platform/src/lib.rs').read_text()
        services = re.findall(r'"(com\.[^"\n]+\.providers)"', platform)
        self.assertEqual(services, ['com.autodict.desktop.providers'])
        self.assertRegex(platform, r'const CREDENTIAL_SERVICE: &str = "com\.autodict\.desktop\.providers";')
        password_services = re.findall(r'(?:get|set|delete)_generic_password\(\s*([^,\s]+)', platform)
        self.assertEqual(password_services, ['CREDENTIAL_SERVICE'] * 3)
        self.assertIn('.service(CREDENTIAL_SERVICE)', platform)

    def test_release_dmg_upload_works_from_a_renamed_checkout(self):
        with tempfile.TemporaryDirectory() as temp:
            checkout = Path(temp) / 'yurelay'
            scripts = checkout / 'scripts'
            scripts.mkdir(parents=True)
            shutil.copy2(ROOT / 'scripts/release-assets.sh', scripts)
            bundle = checkout / 'target/aarch64-apple-darwin/release/bundle'
            dmg = bundle / 'dmg/Yurelay_0.0.0_aarch64.dmg'
            dmg.parent.mkdir(parents=True)
            dmg.write_bytes(b'synthetic disk image')
            tools = Path(temp) / 'tools'
            tools.mkdir()
            gh = tools / 'gh'
            gh.write_text('#!/bin/sh\n[ "$1" = release ] && [ "$2" = upload ]\n')
            gh.chmod(0o755)
            env = dict(os.environ, RUST_TARGET='aarch64-apple-darwin',
                       RELEASE_TAG='v0.0.0', GH_REPO='fixture/local')
            env['PATH'] = str(tools) + os.pathsep + env['PATH']
            subprocess.run(['bash', str(scripts / 'release-assets.sh')],
                           cwd=temp, env=env, check=True, capture_output=True)
            self.assertEqual(list(bundle.rglob('*.app.tar.gz')), [])
            checksums = (bundle / 'SHA256SUMS-aarch64-apple-darwin.txt').read_text()
            self.assertIn(dmg.name, checksums)
            self.assertEqual(len(checksums.splitlines()), 1)
            dmg.unlink()
            missing = subprocess.run(['bash', str(scripts / 'release-assets.sh')],
                                     cwd=temp, env=env, capture_output=True)
            self.assertNotEqual(missing.returncode, 0)


if __name__ == '__main__':
    unittest.main()
