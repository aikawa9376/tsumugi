#!/usr/bin/env bash
set -euo pipefail

task_root=$(cd "$(dirname "$0")/../.." && pwd)
task_prefix="${HOME}/.local"
task_binary="${CARGO_TARGET_DIR:-$task_root/target}/release/tsumugi"
while (($#)); do
    case "$1" in
        --prefix|--binary)
            if (($# < 2)); then echo "missing value for $1" >&2; exit 2; fi
            case "$1" in
                --prefix) task_prefix=$2 ;;
                --binary) task_binary=$2 ;;
            esac
            shift 2 ;;
        *) echo "usage: install.sh [--prefix PATH] [--binary PATH]" >&2; exit 2 ;;
    esac
done
if [[ ! -x "$task_binary" ]]; then
    echo "Build Tsumugi first: cargo build --release --locked -p tsumugi --bin tsumugi" >&2
    exit 1
fi
# Resolve the prefix before writing Exec: launchers do not expand shell paths.
mkdir -p "$task_prefix"
task_prefix=$(cd "$task_prefix" && pwd)
install -Dm755 "$task_binary" "$task_prefix/bin/tsumugi"
install -Dm644 "$task_root/assets/logo/tsumugi-icon.svg" "$task_prefix/share/icons/hicolor/scalable/apps/tsumugi.svg"
bash "$task_root/assets/release/install-notices.sh" "$task_prefix/share/licenses/tsumugi"
python3 - "$task_prefix" <<'PYTHON'
import pathlib, sys
prefix = pathlib.Path(sys.argv[1])
# Escape Desktop Entry Exec syntax, which is not shell syntax.
program = str(prefix / 'bin/tsumugi').replace('%', '%%')
backslash = chr(92)
for char in [backslash, '"', '`', '$']:
    program = program.replace(char, backslash + char)
# Desktop Entry value escaping is applied after Exec argument quoting.
program = program.replace(backslash, backslash * 2)
entry = prefix / 'share/applications/tsumugi.desktop'
entry.parent.mkdir(parents=True, exist_ok=True)
entry.write_text('[Desktop Entry]\nType=Application\nName=Tsumugi\nComment=Agent workspace with Neovim prompts\nExec="' + program + '"\nIcon=tsumugi\nTerminal=false\nCategories=Development;\nKeywords=AI;Agent;Terminal;Neovim;\n')
PYTHON
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$task_prefix/share/applications"
fi
printf 'Installed Tsumugi in %s\n' "$task_prefix"
