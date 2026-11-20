# Building and releases

Tsumugi is developed primarily on Linux. `README.md` lists desktop libraries
and runtime helpers. `nix develop` provides the development tools; alternatively
install Rust, just, Neovim, Node.js and Ghostscript alongside those libraries.
The Rust toolchain and bundled Neovim are pinned in the repository.

On macOS, install the Xcode Command Line Tools (`xcode-select --install`) and
Rust before building. The system webview replaces Linux's GTK/WebKitGTK
libraries, and PDF rendering uses PDFKit rather than Ghostscript. The Nix
development shell supports both `aarch64-darwin` (Apple Silicon) and
`x86_64-darwin` (Intel). Without Nix, install just, Neovim, Node.js,
Python 3 and cargo-about as well as Rust to run the complete quality gate.

Run `just ci` before submitting a change. It builds a version without the
desktop runtime last, so rebuild `cargo build --locked -p tsumugi` before
launching `target/debug/tsumugi` directly. `just run` selects the desktop build.

## Packaging

`just bundle-linux VERSION` builds a Linux archive with its installer, desktop
icon, full license notices and a source-code reference. Build on the oldest
Linux distribution you intend to support: the executable links system GTK,
WebKitGTK and other libraries and is not a universal static binary.

On macOS, `just build-macos` builds `target/macos/Tsumugi.app` and verifies its
signature, executable startup and bundled Neovim runtime with isolated user
state. `just bundle VERSION` also writes the disk image. Build outside the Nix
shell for distribution: the image rejects executables linked to `/nix/store`.
Both Apple Silicon and Intel packages are built natively in CI, with distinct
architecture names; the app is not a universal binary. Windows remains a
source build.

Default bundles are signed ad hoc and are not notarized. A maintainer can
provide their own signing identity through `TSUMUGI_SIGN_IDENTITY`; the bundle
workflow's `sign` input imports the identity from its release environment.
No signing account is required for a local build. macOS may ask users to allow
an unnotarized app in Privacy & Security.

A published GitHub Release builds artifacts in this repository, attaches their
checksums and includes a source archive with vendored Cargo dependencies.
The release version is stamped from the release tag; no external Homebrew tap,
upstream signing account or private build cache is required.

## Rebuilding and relinking

The application is MIT licensed; dependency licenses are listed separately.
To rebuild with a modified LGPL `nvim-rs`, unpack the matching source archive,
modify its vendored source and run `cargo build --release --locked -p tsumugi`.
The included `.cargo/config.toml` selects the vendored crates. The complete
application source, lockfile and build scripts are included so a modified
library can be relinked into the application. System libraries and the Rust
toolchain are prerequisites listed above. A modified macOS app can be signed
ad hoc using the normal bundle script.

## Repository identity

Application commands, internal packages and advanced environment variables use
`tsumugi`, `tsumugi-*` and `TSUMUGI_*`. Old pre-release terminal tags can still
be read so existing local terminal resources remain discoverable. Current
writes use the Tsumugi namespace. Source attribution lives in
`ACKNOWLEDGEMENTS.md` and the original notices remain in `LICENSE`.
