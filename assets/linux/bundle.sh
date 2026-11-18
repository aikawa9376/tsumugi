#!/usr/bin/env bash
# Package an already-built executable with its installer and complete notices.
set -euo pipefail
version=${1:?usage: bundle.sh VERSION [DIST]}
dist=${2:-dist}
root=$(cd "$(dirname "$0")/../.." && pwd)
binary="${CARGO_TARGET_DIR:-$root/target}/release/tsumugi"
arch=$(uname -m)
name="tsumugi_${version}_linux_${arch}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage="$work/$name"
mkdir -p "$stage/assets/linux" "$stage/assets/logo" "$stage/assets/release" "$stage/docs" "$dist"
cp "$binary" "$stage/tsumugi"
cp "$root/assets/linux/install.sh" "$stage/assets/linux/"
cp "$root/assets/logo/"*.svg "$stage/assets/logo/"
cp "$root/assets/release/install-notices.sh" "$stage/assets/release/"
cp "$root/README.md" "$root/LICENSE" "$root/ACKNOWLEDGEMENTS.md" "$root/THIRD-PARTY-LICENSES.txt" "$stage/"
cp -R "$root/docs/." "$stage/docs/"
bash "$root/assets/release/install-notices.sh" "$stage/notices"
cat > "$stage/install.sh" <<'INSTALL'
#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")" && pwd)
exec bash "$root/assets/linux/install.sh" --binary "$root/tsumugi" "$@"
INSTALL
chmod +x "$stage/install.sh"
printf 'Source and rebuild instructions: https://github.com/aikawa9376/tsumugi/releases/tag/v%s\n' "$version" > "$stage/SOURCE.txt"
tar -czf "$dist/$name.tar.gz" -C "$work" "$name"
printf '%s\n' "$dist/$name.tar.gz"
