"""Exercise the packaged programs without relying on a developer's environment."""
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest


VERIFY = Path(__file__).with_name('verify_bundle.py')
NEOVIM = os.environ.get('TSUMUGI_NVIM') or shutil.which('nvim')


@unittest.skipIf(os.name == 'nt', 'the macOS bundle contains Unix executables')
class BundleStartup(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory(prefix='tsumugi bundle 日本語 ')
        self.addCleanup(self.work.cleanup)
        self.app = Path(self.work.name) / 'Tsumugi.app'
        self.contents = self.app / 'Contents'
        self.program('MacOS/tsumugi', "print('tsumugi 0.0.0-test')")
        self.program('Resources/neovim/bin/nvim', 'pass')
        runtime = self.contents / 'Resources/neovim/share/nvim/runtime'
        runtime.mkdir(parents=True)
        (runtime / 'doc').mkdir()
        (runtime / 'doc/help.txt').write_text('Neovim help', encoding='utf-8')
        with (self.contents / 'Info.plist').open('wb') as file:
            plistlib.dump({'CFBundleExecutable': 'tsumugi'}, file)

    def program(self, path, body):
        target = self.contents / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(f'#!{sys.executable}\n{body}\n', encoding='utf-8')
        target.chmod(0o755)

    def verify(self):
        # A calling Neovim and a selected workspace must not contaminate the
        # app's startup check or cause it to contact the user's live hosts.
        env = dict(os.environ, VIM='/invalid/outer/vim', VIMRUNTIME='/invalid/runtime',
                   TSUMUGI_INSTANCE='invalid/instance', TSUMUGI_HOST='/invalid/host')
        return subprocess.run([sys.executable, str(VERIFY), str(self.app)], env=env,
                              capture_output=True, text=True, timeout=15)

    def test_programs_start_with_isolated_state_and_without_the_outer_editor(self):
        self.program('MacOS/tsumugi', """import os
from pathlib import Path
assert not any(key.startswith('TSUMUGI_') for key in os.environ)
assert 'VIM' not in os.environ and 'VIMRUNTIME' not in os.environ
assert Path(os.environ['HOME']).is_dir()
assert os.environ['HOME'] != str(Path(__file__).parents[3])
assert os.environ['PATH'] == os.defpath
print('tsumugi 0.0.0-test')""")
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_an_app_that_cannot_start_is_rejected(self):
        self.program('MacOS/tsumugi', 'raise SystemExit(7)')
        self.assertNotEqual(self.verify().returncode, 0)

    def test_a_non_tsumugi_executable_is_rejected(self):
        self.program('MacOS/tsumugi', "print('another app')")
        self.assertNotEqual(self.verify().returncode, 0)

    def test_a_broken_bundled_neovim_is_rejected(self):
        self.program('Resources/neovim/bin/nvim', 'raise SystemExit(8)')
        self.assertNotEqual(self.verify().returncode, 0)

    def test_a_missing_bundled_runtime_is_rejected(self):
        (self.contents / 'Resources/neovim/share/nvim/runtime/doc/help.txt').unlink()
        self.assertNotEqual(self.verify().returncode, 0)

    @unittest.skipUnless(NEOVIM, 'requires Neovim to exercise runtime discovery')
    def test_a_runtime_outside_the_bundle_is_rejected(self):
        self.program('Resources/neovim/bin/nvim',
                     f'import os, sys\nos.execv({NEOVIM!r}, [{NEOVIM!r}, *sys.argv[1:]])')
        self.assertNotEqual(self.verify().returncode, 0)

    @unittest.skipUnless(NEOVIM, 'requires Neovim to exercise runtime discovery')
    def test_neovim_can_load_the_runtime_carried_by_the_bundle(self):
        probe = Path(self.work.name) / 'runtime.lua'
        probe.write_text('io.stdout:write(vim.env.VIMRUNTIME)', encoding='utf-8')
        env = {key: value for key, value in os.environ.items()
               if key not in ('VIM', 'VIMRUNTIME')}
        result = subprocess.run([NEOVIM, '--headless', '--clean', '-u', 'NONE', '-l', str(probe)],
                                env=env, capture_output=True, text=True, check=True, timeout=10)
        runtime = self.contents / 'Resources/neovim/share/nvim/runtime'
        shutil.rmtree(runtime)
        runtime.symlink_to(result.stdout.strip(), target_is_directory=True)
        self.program('Resources/neovim/bin/nvim',
                     f'import os, sys\nos.execv({NEOVIM!r}, [{NEOVIM!r}, *sys.argv[1:]])')
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_missing_app_executable_is_rejected(self):
        (self.contents / 'MacOS/tsumugi').unlink()
        self.assertNotEqual(self.verify().returncode, 0)


if __name__ == '__main__':
    unittest.main()
