#!/usr/bin/env bash
# Keep the same attribution set in desktop installs, app bundles and archives.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
destination=${1:?usage: install-notices.sh DESTINATION}
mkdir -p "$destination"
if [[ -f "$root/notices/LICENSE" ]]; then
    cp -R "$root/notices/." "$destination/"
    exit 0
fi
cp "$root/LICENSE" "$root/ACKNOWLEDGEMENTS.md" "$root/THIRD-PARTY-LICENSES.txt" "$destination/"
for component in fonts neovim; do
    mkdir -p "$destination/$component"
    cp "$root/assets/$component/"*LICENSE* "$destination/$component/"
done
cp "$root/assets/fonts/OFL.txt" "$destination/fonts/"
for component in vim-nerdfont vim-glyph-palette; do
    mkdir -p "$destination/file-icons/$component"
    cp "$root/assets/file-icons/$component/"LICENSE* "$destination/file-icons/$component/"
done
mkdir -p "$destination/tabler" "$destination/arto-frontend" "$destination/themes"
cp "$root/crates/core/icons/tabler/LICENSE" "$destination/tabler/"
cp "$root/assets/arto-frontend/"*.LICENSE.txt "$destination/arto-frontend/"
cp "$root/crates/host/themes/LICENSES/"* "$destination/themes/"
