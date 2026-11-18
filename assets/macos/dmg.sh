#!/usr/bin/env bash
# Put the Tsumugi.app bundle.sh assembled into the disk image a GitHub Release
# carries, beside a link to /Applications:
#
#   dmg.sh <version> <dist>
#
# The release runs this with the tag's version; CI runs it with a
# placeholder, so a release is never its first run.
set -euo pipefail
version=$1
dist=$2
root=$(cd "$(dirname "$0")/../.." && pwd)
target="${CARGO_TARGET_DIR:-$root/target}"
app="$target/macos/Tsumugi.app"

# A build made in the Nix development shell may refer into /nix/store, which
# no machine without that store has; it must not be what a release ships.
linked=$(otool -L "$app/Contents/MacOS/tsumugi")
if grep -q /nix/store <<<"$linked"; then
  echo "error: Tsumugi.app refers into /nix/store; build it without the Nix shell" >&2
  echo "$linked" >&2
  exit 1
fi

case "$(uname -m)" in
arm64 | aarch64) arch=aarch64 ;;
*) arch=$(uname -m) ;;
esac

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/Tsumugi" "$dist"
# ditto keeps the signatures and the symbolic links Neovim's folders hold.
ditto "$app" "$stage/Tsumugi/Tsumugi.app"
ln -s /Applications "$stage/Tsumugi/Applications"
hdiutil create -quiet -volname Tsumugi -srcfolder "$stage/Tsumugi" -ov \
  -format UDZO "$dist/Tsumugi_${version}_${arch}.dmg"
ls -l "$dist"
