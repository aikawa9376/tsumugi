#!/usr/bin/env bash
# Include the complete Rust sources needed to rebuild and relink a release.
set -euo pipefail
version=${1:?usage: source.sh VERSION [DIST]}
dist=${2:-dist}
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
name="tsumugi_${version}_source"
stage="$work/$name"
mkdir -p "$stage" "$dist"
# Only committed source enters a public archive; local editor/session data does not.
git -C "$root" archive HEAD | tar -C "$stage" -xf -
perl "$stage/assets/release/stamp-version.pl" "$version" "$stage"
mkdir -p "$stage/.cargo"
# Beside the tasks' alias the repository keeps there.
(cd "$stage" && cargo vendor --locked vendor >> .cargo/config.toml)
tar -czf "$dist/$name.tar.gz" -C "$work" "$name"
printf '%s\n' "$dist/$name.tar.gz"
