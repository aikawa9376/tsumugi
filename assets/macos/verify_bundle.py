"""Start an app bundle's programs without the build machine's user settings."""
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile


def verify(app):
    contents = app.resolve(strict=True) / 'Contents'
    with (contents / 'Info.plist').open('rb') as file:
        executable = plistlib.load(file)['CFBundleExecutable']
    if executable != 'tsumugi':
        raise ValueError('the app must launch tsumugi')
    neovim = contents / 'Resources/neovim'
    runtime = neovim / 'share/nvim/runtime'
    if not (runtime / 'doc/help.txt').is_file():
        raise ValueError('the bundled Neovim runtime is missing')

    with tempfile.TemporaryDirectory(prefix='tsumugi-bundle-check-') as directory:
        work = Path(directory)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith('TSUMUGI_') and key not in ('VIM', 'VIMRUNTIME')}
        env.update(HOME=directory, PATH=os.defpath, ZDOTDIR=str(work / 'zsh'),
                   XDG_CONFIG_HOME=str(work / 'config'), XDG_STATE_HOME=str(work / 'state'),
                   XDG_DATA_HOME=str(work / 'data'), XDG_CACHE_HOME=str(work / 'cache'))

        def run(*args):
            return subprocess.run(args, env=env, cwd=directory, stdin=subprocess.DEVNULL,
                                  capture_output=True, text=True, check=True, timeout=30)

        version = run(str(contents / 'MacOS' / executable), '--version').stdout.strip()
        if not version.startswith('tsumugi '):
            raise ValueError(f'the app did not report its version: {version!r}')
        # An installed Neovim must not hide missing files in the app's copy.
        env['TSUMUGI_BUNDLE_RUNTIME'] = str(runtime)
        check = work / 'runtime.lua'
        check.write_text(
            'assert(vim.uv.fs_realpath(vim.env.VIMRUNTIME) == '
            'vim.uv.fs_realpath(vim.env.TSUMUGI_BUNDLE_RUNTIME), "runtime outside app bundle")\n',
            encoding='utf-8')
        # -l propagates a Lua failure; +lua followed by +qa can still exit 0.
        run(str(neovim / 'bin/nvim'), '--headless', '--clean', '-u', 'NONE', '-i', 'NONE',
            '-n', '-l', str(check))
        print(f'{version}: app and bundled Neovim start successfully')


if __name__ == '__main__':
    try:
        verify(Path(sys.argv[1]))
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f'bundle check failed: {error}', file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError):
            print(error.stderr, file=sys.stderr)
        sys.exit(1)
